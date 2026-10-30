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
    produce: BTreeMap<String, u32>,
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
        if !grass
            && !matches!(selection, FloraPaintSelection::Species(i) if species::species()[i as usize].is_root_crop())
        {
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
    #[serde(default)]
    pub crops: BTreeMap<String, u32>,
    #[serde(deserialize_with = "decode_stock")]
    stock: BTreeMap<String, u32>,
    harvested: BTreeSet<HarvestKey>,
}
fn decode_stock<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<BTreeMap<String, u32>, D::Error> {
    let mut stock = BTreeMap::<String, u32>::deserialize(d)?;
    if let Some(old) = stock.remove("sapling/weeping_willow") {
        let count = stock.entry("sapling/london_plane".into()).or_default();
        *count = count
            .checked_add(old)
            .ok_or_else(|| serde::de::Error::custom("sapling stock overflow"))?;
    }
    Ok(stock)
}

impl Default for GardenEconomy {
    fn default() -> Self {
        Self {
            coins: PRICES.starting_coins,
            apples: 0,
            crops: BTreeMap::new(),
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
            self.crops
                .iter()
                .all(|(id, n)| PRICES.produce.contains_key(id) && *n <= MAX_STOCK),
            "invalid crop stock"
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
    pub fn crop_price(id: &str) -> Option<u32> {
        PRICES.produce.get(id).copied()
    }
    pub fn sell_crop(&mut self, id: &str, count: u32) -> Result<u32> {
        let price = Self::crop_price(id).ok_or_else(|| anyhow::anyhow!("Unknown crop"))?;
        let owned = self.crops.get(id).copied().unwrap_or(0);
        ensure!(count > 0 && count <= owned, "No crop to sell");
        let proceeds = count
            .checked_mul(price)
            .ok_or_else(|| anyhow::anyhow!("Sale too large"))?;
        let coins = self
            .coins
            .checked_add(proceeds)
            .filter(|n| *n <= MAX_COINS)
            .ok_or_else(|| anyhow::anyhow!("Wallet is full"))?;
        self.coins = coins;
        self.crops.insert(id.to_owned(), owned - count);
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
    pub fn collect_plant(
        &mut self,
        species: u32,
        growth: u32,
        take: impl FnOnce() -> bool,
    ) -> Result<bool> {
        let definition = species::species()
            .get(species as usize)
            .ok_or_else(|| anyhow::anyhow!("Unknown plant"))?;
        if definition.is_root_crop() {
            if growth < 255 {
                return Ok(false);
            }
            let count = self.crops.get(definition.key).copied().unwrap_or(0);
            ensure!(count < MAX_STOCK, "Crop storage is full");
            if !take() {
                return Ok(false);
            }
            self.crops.insert(definition.key.to_owned(), count + 1);
            return Ok(true);
        }
        let p = product_for(PlantGoods::Seedling(FloraPaintSelection::Species(species)))
            .ok_or_else(|| anyhow::anyhow!("This plant cannot be collected"))?;
        let count = self.count(&p.id);
        ensure!(count < MAX_STOCK, "Plant storage is full");
        if !take() {
            return Ok(false);
        }
        self.stock.insert(p.id.clone(), count + 1);
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
    fn plant_collection_checks_capacity_and_policy_before_taking_exact_owner() {
        let species = species::MODEL_FLOWER_FIRST_SPECIES;
        let product =
            product_for(PlantGoods::Seedling(FloraPaintSelection::Species(species))).unwrap();
        let mut bag = GardenEconomy::default();
        assert!(bag
            .collect_plant(0, 255, || panic!("grass must never be taken"))
            .is_err());
        bag.stock.insert(product.id.clone(), MAX_STOCK);
        assert!(bag
            .collect_plant(species, 255, || panic!(
                "full bag must never remove a plant"
            ))
            .is_err());
        bag.stock.clear();
        assert!(!bag.collect_plant(species, 255, || false).unwrap());
        assert_eq!(bag.count(&product.id), 0);
        assert!(bag.collect_plant(species, 255, || true).unwrap());
        assert_eq!(bag.count(&product.id), 1);
        bag.validate().unwrap();
    }
    #[test]
    fn ginkgo_saplings_are_purchasable_and_stock_survives_restore() {
        let goods = PlantGoods::Sapling(TreeSpecies::Ginkgo);
        let item = product_for(goods).unwrap();
        assert_eq!(item.id, "sapling/ginkgo");
        assert_eq!(item.price, 35);
        let mut bag = GardenEconomy::default();
        let coins = bag.coins;
        bag.buy(&item.id).unwrap();
        assert_eq!(bag.coins, coins - item.price);
        assert_eq!(bag.count(&item.id), 1);
        let saved: GardenEconomy =
            serde_json::from_slice(&serde_json::to_vec(&bag).unwrap()).unwrap();
        assert_eq!(saved, bag);
        saved.validate().unwrap();
    }

    #[test]
    fn blackberry_can_be_bought_and_replanted_but_does_not_sell_decorative_fruit() {
        let index = species::species()
            .iter()
            .position(|s| s.key == "blackberry")
            .unwrap() as u32;
        let selection = FloraPaintSelection::Species(index);
        assert_eq!(
            product("seeds/blackberry").unwrap().goods,
            PlantGoods::Seeds(selection)
        );
        assert_eq!(
            product("seedling/blackberry").unwrap().goods,
            PlantGoods::Seedling(selection)
        );
        let mut bag = GardenEconomy::default();
        bag.buy("seeds/blackberry").unwrap();
        assert!(bag.collect_plant(index, 255, || true).unwrap());
        assert_eq!(bag.count("seedling/blackberry"), 1);
        assert!(!bag.crops.contains_key("blackberry"));
        assert!(GardenEconomy::crop_price("blackberry").is_none());
    }

    #[test]
    fn allium_uses_flower_seed_and_reusable_seedling_ownership() {
        let allium = species::species()
            .iter()
            .position(|s| s.key == "allium")
            .unwrap() as u32;
        let selection = FloraPaintSelection::Species(allium);
        assert_eq!(
            product("seeds/allium").unwrap().goods,
            PlantGoods::Seeds(selection)
        );
        assert_eq!(
            product("seedling/allium").unwrap().goods,
            PlantGoods::Seedling(selection)
        );
        let mut bag = GardenEconomy::default();
        bag.buy("seeds/allium").unwrap();
        assert_eq!(bag.count("seeds/allium"), PRICES.flower_seed_packet_size);
        assert!(bag.collect_plant(allium, 255, || true).unwrap());
        assert_eq!(bag.count("seedling/allium"), 1);
        assert!(!bag.crops.contains_key("allium"));
        let saved: GardenEconomy =
            serde_json::from_str(&serde_json::to_string(&bag).unwrap()).unwrap();
        assert_eq!(saved, bag);
        saved.validate().unwrap();
    }

    #[test]
    fn carrots_are_seed_grown_mature_produce_not_reusable_seedlings() {
        let carrot = species::species()
            .iter()
            .position(|s| s.key == "carrot")
            .unwrap() as u32;
        assert!(product("seeds/carrot").is_some());
        assert!(product("seedling/carrot").is_none());
        assert!(!species::species()[carrot as usize].provides_nectar());
        let mut bag = GardenEconomy::default();
        assert!(!bag
            .collect_plant(carrot, 254, || panic!("immature crop removed"))
            .unwrap());
        bag.crops.insert("carrot".into(), MAX_STOCK);
        assert!(bag
            .collect_plant(carrot, 255, || panic!("full crop bag"))
            .is_err());
        bag.crops.clear();
        assert!(!bag.collect_plant(carrot, 255, || false).unwrap());
        assert!(bag.collect_plant(carrot, 255, || true).unwrap());
        assert_eq!(bag.crops.get("carrot"), Some(&1));
        let saved: GardenEconomy =
            serde_json::from_str(&serde_json::to_string(&bag).unwrap()).unwrap();
        assert_eq!(bag, saved);
        saved.validate().unwrap();
        let coins = bag.coins;
        assert_eq!(bag.sell_crop("carrot", 1).unwrap(), 3);
        assert_eq!(bag.coins, coins + 3);
        let before = bag.clone();
        assert!(bag.sell_crop("carrot", 1).is_err());
        assert!(bag.sell_crop("unknown", 1).is_err());
        assert_eq!(bag, before);
        let mut legacy = serde_json::to_value(&bag).unwrap();
        legacy.as_object_mut().unwrap().remove("crops");
        let legacy: GardenEconomy = serde_json::from_value(legacy).unwrap();
        assert!(legacy.crops.is_empty());
    }

    #[test]
    fn retired_willow_stock_becomes_plane_stock_without_losing_goods() {
        let mut value = serde_json::to_value(GardenEconomy::default()).unwrap();
        value["stock"] = serde_json::json!({"sapling/weeping_willow":2,"sapling/london_plane":3});
        let saved: GardenEconomy = serde_json::from_value(value).unwrap();
        saved.validate().unwrap();
        assert_eq!(saved.count("sapling/london_plane"), 5);
        assert_eq!(saved.count("sapling/weeping_willow"), 0);
        assert!(!catalog().iter().any(|p| p.id.contains("willow")));
    }
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
