# Harvest, storage and the garden shop

- **Harvest** (stable toolbar slot 22), or **E** while aiming, collects one ripe
  attached or loose apple. Immature apples are not collectible.
- **B** / the wallet's **Shop** button opens the shop. B or Esc closes it; the
  modal releases the pointer and blocks world tools. Holding Esc does not repeat
  through closing the shop into the game's global Quit shortcut.
- Apples remain in persistent storage until sold. The shop sells grass seed
  packets, seeds/seedlings for every current flower, and all three tree saplings.
  Use **Grow** with the corresponding variety to plant purchased stock.
- In finite mode a placement spends exactly one plant, not a brush-sized guess.
  Seedlings are used before seeds, starting at growth 96/255 instead of zero.
  A tree uses the normal young-tree lifecycle described in `garden-saplings.md`.
- The code-only `INFINITE_MATERIALS` switch in `src/app/core/material_supply.rs`
  currently fixes this build to free planting without changing real stock.
  The Debug Material Supply section is removed; old saved toggles are ignored. Unlimited flora retains the existing painting
  brush. Purchases themselves still spend real coins in either mode. Plow and
  shovel retain their finite soil ownership; this build policy does not create soil in
  either movement tool.

## Authored balance

`config/garden-shop.toml` is the single embedded balance source; rebuild after
editing it. Existing saves keep their actual wallet balance.

| Item | Current price |
|---|---:|
| Starting wallet | 100 coins |
| Sell one apple | +3 coins |
| 32 grass seeds | 4 coins |
| 5 flower seeds | 10 coins |
| One flower seedling | 6 coins |
| Apple / Pine / Willow sapling | 30 / 25 / 35 coins |

Stock IDs derive from existing stable flora keys and serialized tree species,
not toolbar positions. There are 18 purchase entries. The native list scrolls;
tree saplings follow the flower entries.

## Ownership and persistence

`GardenEconomy` owns coins, stored apples, purchased stock and harvested
`(tree ID, fruit ID)` receipts. Purchases/sales check funds, capacity and arithmetic
before changing either side. Fruit collection first checks duplicate/capacity
constraints, then takes the live physics owner with an infallible callback and
credits storage without intervening fallible work. A later render failure cannot
refund the apple into the world or enable another credit.

Retiring an apple removes its registry entry and any dynamic body. Late release
tickets find no remaining owner. Every tree publication, including growth,
manual rebuilding and restoration, filters the persistent receipts. Removing a
tree prunes its receipts; ordinary planting never reuses that tree ID.

The garden snapshot has a backward-compatible default `economy` field. Older
saves start with an empty plant/apple store and the initial wallet. Restore installs
the new ledger before publishing its trees, and restores the previous ledger if
the tree transaction fails. Validation rejects receipts for nonexistent trees.
This is local game persistence, not an anti-cheat or tamper-proof save format.

Finite planting validates the actual two-cell atlas contact. Blocked, duplicate,
unsupported and out-of-world attempts do not debit stock. Authored-flora capacity
is checked before insertion. New grass appends its eight-byte instance and
four-byte hydration value without reading/replacing the neighboring grass stream;
its CPU ownership/count becomes visible only after both hidden-tail uploads
succeed. Normal grass coverage and hydration publication receive the new root.
Stock transfers immediately after committed insertion, before later publication.

## Native evidence

A Release fixture exercised real purchase, insertion, fruit-picking/retirement
and publication methods. It began with 100 coins, bought two Apple saplings,
one flower seed packet, one seedling and one grass packet; planted finite and
unlimited-debug trees; planted a seedling, seed and grass root; collected two
ripe apples; and sold one.

Final state, preserved by an actual binary save/restart:

- **23 coins, 1 stored apple, 2 harvest receipts**;
- **1 owned sapling, 4 sunflower seeds, 31 grass seeds**;
- **3 live flora roots, including 2 authored flowers, and 2 trees**;
- no collected apple in the restored physics registry, also after manual tree
  rebuilding before saving.

Empty stock, invalid positions, duplicate flower/grass planting, unripe fruit,
a foreground occlusion distance, repeated fruit identity, insufficient funds and
overselling were rejected without changing ownership. The unlimited planting
check left the real inventory byte-for-byte unchanged. A separate live-frame
review used normal kick/release handoff and physics, then picked up one fallen
apple after 60 frames and verified that its dynamic owner was retired.

`target/garden-shop.png` is an actual native UI capture. Other captures are
`target/garden-harvest-before.png`, `target/garden-harvest.png`, and
`target/garden-fallen-harvest.png`. The native fixtures call production methods;
they are not physical mouse/keyboard play-throughs. Headless egui tests separately
click the actual purchase button with/without funds, and input tests cover the
primary-click action and shop shortcuts. No subjective interaction approval is
claimed.

The initial planting fixture incorrectly combined a height-only CPU query result
with its original x/z. That CPU ray sanitizes vertical directions and had drifted
into a neighboring column. Keeping the entire returned contact fixed the fixture;
the production path already retains the entire captured GPU contact. The strict
atlas substrate check was not weakened. A second fixture correction respects the
actual nearest picked fruit rather than assuming that aiming toward one member
of an overlapping cluster must select that particular identity.

```sh
env -u WAYLAND_DISPLAY RE_FLORA_ECONOMY_REVIEW=trade RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-save target/garden-economy.rflterrain \
  --screenshot garden-arrival target/garden-harvest.png \
  --screenshot-delay 2 --auto-exit 3
env -u WAYLAND_DISPLAY RE_FLORA_ECONOMY_REVIEW=load RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-load target/garden-economy.rflterrain --auto-exit 1
env -u WAYLAND_DISPLAY RE_FLORA_ECONOMY_REVIEW=shop RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/garden-shop.png \
  --screenshot-delay 2 --auto-exit 3
# Other modes: before (uncollected crop), fallen (normal live release + collection).
```

Formatting/check, economy/UI, physics harvesting, player-tool, toolbar,
terrain-persistence, vegetation, grass-coverage and hydration tests passed.
Release build and ordinary hidden muted smoke passed. Final trade/load/shop/fallen
logs contain no ERROR/panic/VUID; no generated files or saved GUI defaults changed.

## Deliberate limits

- One crop per stable fruit identity; no recurring harvest seasons or honey economy.
  Existing behavior reattaches **uncollected** loose fruit on load rather than
  persisting its physics trajectory. Collected fruit does not return.
- Picking uses bounded sphere proxies and the existing attachment-pose API, with
  terrain/wood occlusion, two-world-unit reach and at most 256 nearby candidates
  per click. Extreme debug sway or dense overlapping crops are not exhaustively
  tested. This is not exact per-pixel apple picking or a dense-garden performance
  acceptance result. There are no idle harvesting pose readbacks.
- Storage is bounded at one million of each good/apple, one billion coins and
  65,536 harvest receipts. Full storage rejects a transfer before retiring fruit.
- This is the plant/fruit economy. It does not add prices to all construction
  tools, persist the pre-existing generic voxel backpack, or implement plant
  refunds/seed harvesting. Manual Debug tree editing remains a creative override.
- No new GPU-allocation failure injection or subjective manual input test is
  claimed; existing transactional tree-publication fault tests remain in use.
