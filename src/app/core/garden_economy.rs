//! Persistent garden goods and atomic exchange rules. World entities transfer
//! ownership through an infallible take callback before any presentation work.
use super::plant_catalog::{PlantVariety, TreeSpecies};
use crate::flora::species::{self, FloraPaintSelection};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

mod review;
mod ui;
mod world;
pub(super) use ui::{draw_garden_shop, shop_open_after_key, ShopAction};

const MAX_STOCK: u32 = 1_000_000;
const MAX_COINS: u32 = 1_000_000_000;
const MAX_HARVEST_RECEIPTS: usize = 65_536;

#[derive(Deserialize)]
struct Prices {
    starting_coins: u32,
    apple_sell_price: u32,
    grass_packet_price: u32,
    grass_packet_size: u32,
    flower_seed_packet_price: u32,
    flower_seed_packet_size: u32,
    flower_seedling_price: u32,
    saplings: BTreeMap<String, u32>,
}
static PRICES: LazyLock<Prices> = LazyLock::new(|| {
    let p: Prices = toml::from_str(include_str!("../../../config/garden-shop.toml"))
        .expect("valid garden shop balance data");
    assert!(p.starting_coins <= MAX_COINS && (1..=1000).contains(&p.apple_sell_price));
    assert!([p.grass_packet_size, p.flower_seed_packet_size]
        .iter()
        .all(|n| (1..=1024).contains(n)));
    p
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) struct HarvestKey {
    pub tree: u32,
    pub fruit: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlantGoods {
    Seeds(FloraPaintSelection),
    Seedling(FloraPaintSelection),
    Sapling(TreeSpecies),
}
#[derive(Clone, Debug)]
pub(super) struct Product {
    pub id: String,
    pub label: String,
    pub price: u32,
    pub quantity: u32,
    pub goods: PlantGoods,
}

fn tree_key(species: TreeSpecies) -> String {
    serde_json::to_value(species)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
static CATALOG: LazyLock<Vec<Product>> = LazyLock::new(|| {
    let p = &*PRICES;
    let mut result = Vec::new();
    for &selection in species::PLAYER_FLORA_PAINT_SELECTIONS {
        let (key, label) = match selection {
            FloraPaintSelection::GrassMix => ("grass_mix", "Grass mix"),
            FloraPaintSelection::Species(i) => {
                let d = &species::species()[i as usize];
                (d.key, d.display_name)
            }
            FloraPaintSelection::ClimbingVine => continue,
        };
        let grass = selection == FloraPaintSelection::GrassMix;
        let quantity = if grass {
            p.grass_packet_size
        } else {
            p.flower_seed_packet_size
        };
        result.push(Product {
            id: format!("seeds/{key}"),
            label: format!("{label} seeds ×{quantity}"),
            price: if grass {
                p.grass_packet_price
            } else {
                p.flower_seed_packet_price
            },
            quantity,
            goods: PlantGoods::Seeds(selection),
        });
        if !grass {
            result.push(Product {
                id: format!("seedling/{key}"),
                label: format!("{label} seedling"),
                price: p.flower_seedling_price,
                quantity: 1,
                goods: PlantGoods::Seedling(selection),
            });
        }
    }
    for species in TreeSpecies::PLANTABLE {
        let key = tree_key(species);
        result.push(Product {
            id: format!("sapling/{key}"),
            label: format!("{} sapling", species.label()),
            price: *p.saplings.get(&key).expect("sapling price"),
            quantity: 1,
            goods: PlantGoods::Sapling(species),
        });
    }
    assert!(result.iter().all(|p| (1..=MAX_COINS).contains(&p.price)));
    result
});
pub(super) fn catalog() -> &'static [Product] {
    &CATALOG
}
pub(super) fn product(id: &str) -> Option<&'static Product> {
    catalog().iter().find(|p| p.id == id)
}
pub(super) fn product_for(goods: PlantGoods) -> Option<&'static Product> {
    catalog().iter().find(|p| p.goods == goods)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct GardenEconomy {
    pub coins: u32,
    pub apples: u32,
    stock: BTreeMap<String, u32>,
    harvested: BTreeSet<HarvestKey>,
}
impl Default for GardenEconomy {
    fn default() -> Self {
        Self {
            coins: PRICES.starting_coins,
            apples: 0,
            stock: BTreeMap::new(),
            harvested: BTreeSet::new(),
        }
    }
}
impl GardenEconomy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.coins <= MAX_COINS && self.apples <= MAX_STOCK,
            "garden storage exceeds capacity"
        );
        ensure!(
            self.stock.len() <= catalog().len()
                && self
                    .stock
                    .iter()
                    .all(|(id, n)| product(id).is_some() && *n <= MAX_STOCK),
            "invalid purchased plant stock"
        );
        ensure!(
            self.harvested.len() <= MAX_HARVEST_RECEIPTS
                && self.harvested.iter().all(|k| k.tree < u32::MAX),
            "invalid harvested fruit identities"
        );
        Ok(())
    }
    pub fn count(&self, id: &str) -> u32 {
        self.stock.get(id).copied().unwrap_or(0)
    }
    pub fn buy(&mut self, id: &str) -> Result<()> {
        let p = product(id).ok_or_else(|| anyhow::anyhow!("Unknown garden item"))?;
        ensure!(self.coins >= p.price, "Not enough coins");
        let count = self
            .count(id)
            .checked_add(p.quantity)
            .filter(|n| *n <= MAX_STOCK)
            .ok_or_else(|| anyhow::anyhow!("Plant storage is full"))?;
        self.coins -= p.price;
        self.stock.insert(id.to_owned(), count);
        Ok(())
    }
    pub fn sell_apples(&mut self, count: u32) -> Result<u32> {
        ensure!(count > 0 && count <= self.apples, "No apples to sell");
        let proceeds = count
            .checked_mul(PRICES.apple_sell_price)
            .ok_or_else(|| anyhow::anyhow!("Sale exceeds wallet capacity"))?;
        let coins = self
            .coins
            .checked_add(proceeds)
            .filter(|n| *n <= MAX_COINS)
            .ok_or_else(|| anyhow::anyhow!("Wallet is full"))?;
        self.apples -= count;
        self.coins = coins;
        Ok(proceeds)
    }
    pub fn apple_price() -> u32 {
        PRICES.apple_sell_price
    }
    pub fn harvested(&self, key: HarvestKey) -> bool {
        self.harvested.contains(&key)
    }
    pub fn collect(&mut self, key: HarvestKey, take: impl FnOnce() -> bool) -> Result<bool> {
        if self.harvested(key) {
            return Ok(false);
        }
        ensure!(
            self.apples < MAX_STOCK && self.harvested.len() < MAX_HARVEST_RECEIPTS,
            "Harvest storage is full"
        );
        if !take() {
            return Ok(false);
        }
        // No fallible work between removal from the world and credit to storage.
        self.apples += 1;
        self.harvested.insert(key);
        Ok(true)
    }
    pub fn forget_tree(&mut self, tree: u32) {
        self.harvested.retain(|k| k.tree != tree);
    }
    pub fn harvest_receipts(&self) -> impl Iterator<Item = HarvestKey> + '_ {
        self.harvested.iter().copied()
    }
    pub fn planting_supply(&self, variety: PlantVariety) -> Option<&'static Product> {
        let goods = match variety {
            PlantVariety::Tree(s) => PlantGoods::Sapling(s),
            PlantVariety::Flora(s) => {
                if let Some(p) =
                    product_for(PlantGoods::Seedling(s)).filter(|p| self.count(&p.id) > 0)
                {
                    return Some(p);
                }
                PlantGoods::Seeds(s)
            }
        };
        product_for(goods).filter(|p| self.count(&p.id) > 0)
    }
    /// Called immediately after successful entity insertion, before publication.
    /// Admission and commit run on the same App thread without another trade.
    pub fn consume_committed_plant(&mut self, id: &str) {
        let count = self
            .stock
            .get_mut(id)
            .expect("planting requires admitted stock");
        assert!(
            *count > 0,
            "planting stock was reserved by this synchronous operation"
        );
        *count -= 1;
    }
    pub fn stock_summary(&self, variety: PlantVariety, infinite: bool) -> String {
        if infinite {
            return "Debug · unlimited planting".into();
        }
        match self.planting_supply(variety) {
            Some(p) => format!(
                "{} owned · {}",
                self.count(&p.id),
                match p.goods {
                    PlantGoods::Seeds(_) => "seeds",
                    PlantGoods::Seedling(_) => "seedlings first",
                    PlantGoods::Sapling(_) => "saplings",
                }
            ),
            None => "Out of stock · open Shop (B)".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trade_is_exact_and_rejections_leave_all_owned_state_unchanged() {
        let mut e = GardenEconomy::default();
        let p = product("sapling/apple_tree").unwrap();
        e.buy(&p.id).unwrap();
        assert_eq!(e.coins, PRICES.starting_coins - p.price);
        assert_eq!(e.count(&p.id), 1);
        e.consume_committed_plant(&p.id);
        assert_eq!(e.count(&p.id), 0);
        let before = e.clone();
        assert!(e.buy("missing").is_err());
        assert_eq!(e, before);
        e.coins = 0;
        let before = e.clone();
        assert!(e.buy(&p.id).is_err());
        assert_eq!(e, before);
    }
    #[test]
    fn world_transfer_credit_and_saved_receipt_prevent_double_collection() {
        let mut e = GardenEconomy::default();
        let key = HarvestKey { tree: 7, fruit: 42 };
        assert!(!e.collect(key, || false).unwrap());
        assert_eq!(e.apples, 0);
        assert!(e.collect(key, || true).unwrap());
        assert_eq!(e.apples, 1);
        assert!(!e
            .collect(key, || panic!("duplicate must not touch world"))
            .unwrap());
        let mut restored: GardenEconomy =
            serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        restored.validate().unwrap();
        assert!(!restored.collect(key, || true).unwrap());
        assert_eq!(restored.sell_apples(1).unwrap(), PRICES.apple_sell_price);
        assert_eq!(restored.apples, 0);
        assert!(restored.sell_apples(1).is_err());
        assert!(!restored.collect(key, || true).unwrap());
    }
    #[test]
    fn capacities_are_checked_before_resource_transfer() {
        let key = HarvestKey { tree: 1, fruit: 2 };
        let mut e = GardenEconomy {
            apples: MAX_STOCK,
            ..Default::default()
        };
        assert!(e
            .collect(key, || panic!("full storage must not retire fruit"))
            .is_err());
        e.coins = MAX_COINS;
        e.apples = 1;
        let before = e.clone();
        assert!(e.sell_apples(1).is_err());
        assert_eq!(e, before);
        e.stock.insert("sapling/apple_tree".into(), MAX_STOCK);
        let before = e.clone();
        assert!(e.buy("sapling/apple_tree").is_err());
        assert_eq!(e, before);
    }
    #[test]
    fn catalog_uses_stable_species_keys_and_seedlings_have_explicit_priority() {
        let ids = catalog()
            .iter()
            .map(|p| p.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), catalog().len());
        let selection = FloraPaintSelection::Species(species::MODEL_FLOWER_FIRST_SPECIES);
        let seeds = product_for(PlantGoods::Seeds(selection)).unwrap();
        let seedling = product_for(PlantGoods::Seedling(selection)).unwrap();
        let mut e = GardenEconomy::default();
        e.buy(&seeds.id).unwrap();
        e.buy(&seedling.id).unwrap();
        assert_eq!(
            e.planting_supply(PlantVariety::Flora(selection))
                .unwrap()
                .id,
            seedling.id
        );
        e.consume_committed_plant(&seedling.id);
        assert_eq!(
            e.planting_supply(PlantVariety::Flora(selection))
                .unwrap()
                .id,
            seeds.id
        );
        assert!(ids.contains("seeds/sunflower"));
        assert_eq!(
            e.coins,
            PRICES.starting_coins - seeds.price - seedling.price
        );
    }
}
