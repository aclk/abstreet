use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use abstio::MapName;
use abstutil::Timer;
use map_model::osm::RoadRank;
use map_model::{Block, Map, Perimeter, RoadID, RoadSideID};
use widgetry::Color;

use crate::App;

const COLORS: [Color; 6] = [
    Color::BLUE,
    Color::YELLOW,
    Color::GREEN,
    Color::PURPLE,
    Color::PINK,
    Color::ORANGE,
];

/// An opaque ID, won't be contiguous as we adjust boundaries
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NeighborhoodID(usize);

/// Identifies a single / unmerged block, which never changes
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockID(usize);

// Some states want this
impl widgetry::mapspace::ObjectID for NeighborhoodID {}
impl widgetry::mapspace::ObjectID for BlockID {}

#[derive(Clone)]
pub struct Partitioning {
    pub map: MapName,
    neighborhoods: BTreeMap<NeighborhoodID, (Block, Color)>,
    // The single / unmerged blocks never change
    single_blocks: Vec<Block>,

    neighborhood_id_counter: usize,

    // Invariant: This is a bijection, every block belongs to exactly one neighborhood
    block_to_neighborhood: BTreeMap<BlockID, NeighborhoodID>,
}

impl Partitioning {
    /// Only valid before the LTN tool has been activated this session
    pub fn empty() -> Partitioning {
        Partitioning {
            map: MapName::new("zz", "temp", "orary"),
            neighborhoods: BTreeMap::new(),
            single_blocks: Vec::new(),

            neighborhood_id_counter: 0,

            block_to_neighborhood: BTreeMap::new(),
        }
    }

    pub fn seed_using_heuristics(app: &App, timer: &mut Timer) -> Partitioning {
        let map = &app.map;
        timer.start("find single blocks");
        let mut single_blocks = Vec::new();
        let mut single_block_perims = Vec::new();
        for perim in Perimeter::find_all_single_blocks(map) {
            if let Ok(block) = perim.to_block(map) {
                single_block_perims.push(block.perimeter.clone());
                single_blocks.push(block);
            }
        }
        timer.stop("find single blocks");

        timer.start("partition");
        let partitions = Perimeter::partition_by_predicate(single_block_perims, |r| {
            // "Interior" roads of a neighborhood aren't classified as arterial
            map.get_r(r).get_rank() == RoadRank::Local
        });

        let mut merged = Vec::new();
        for perimeters in partitions {
            // If we got more than one result back, merging partially failed. Oh well?
            merged.extend(Perimeter::merge_all(perimeters, false));
        }
        timer.stop("partition");

        timer.start_iter("blockify", merged.len());
        let mut blocks = Vec::new();
        for perimeter in merged {
            timer.next();
            match perimeter.to_block(map) {
                Ok(block) => {
                    blocks.push(block);
                }
                Err(err) => {
                    warn!("Failed to make a block from a merged perimeter: {}", err);
                }
            }
        }

        let mut neighborhoods = BTreeMap::new();
        for block in blocks {
            neighborhoods.insert(NeighborhoodID(neighborhoods.len()), (block, Color::RED));
        }
        let neighborhood_id_counter = neighborhoods.len();
        let mut p = Partitioning {
            map: map.get_name().clone(),
            neighborhoods,
            single_blocks,

            neighborhood_id_counter,
            block_to_neighborhood: BTreeMap::new(),
        };

        // TODO We could probably build this up as we go
        for id in p.all_block_ids() {
            if let Some(neighborhood) = p.neighborhood_containing(id) {
                p.block_to_neighborhood.insert(id, neighborhood);
            } else {
                // TODO What happened? This will break everything downstream.
                error!(
                    "Block doesn't belong to any neighborhood?! {:?}",
                    p.get_block(id).perimeter
                );
            }
        }

        p.recalculate_coloring();
        p
    }

    /// True if the coloring changed
    pub fn recalculate_coloring(&mut self) -> bool {
        let perims: Vec<Perimeter> = self
            .neighborhoods
            .values()
            .map(|pair| pair.0.perimeter.clone())
            .collect();
        let colors = Perimeter::calculate_coloring(&perims, COLORS.len())
            .unwrap_or_else(|| (0..perims.len()).collect());
        let orig_coloring: Vec<Color> = self.neighborhoods.values().map(|pair| pair.1).collect();
        for (pair, color_idx) in self.neighborhoods.values_mut().zip(colors.into_iter()) {
            pair.1 = COLORS[color_idx % COLORS.len()];
        }
        let new_coloring: Vec<Color> = self.neighborhoods.values().map(|pair| pair.1).collect();
        orig_coloring != new_coloring
    }

    // TODO Explain return value
    pub fn transfer_block(
        &mut self,
        map: &Map,
        id: BlockID,
        old_owner: NeighborhoodID,
        new_owner: NeighborhoodID,
    ) -> Result<Option<NeighborhoodID>> {
        assert_ne!(old_owner, new_owner);

        // Is the newly expanded neighborhood a valid perimeter?
        let new_owner_blocks: Vec<BlockID> = self
            .block_to_neighborhood
            .iter()
            .filter_map(|(block, neighborhood)| {
                if *neighborhood == new_owner || *block == id {
                    Some(*block)
                } else {
                    None
                }
            })
            .collect();
        let new_neighborhood_block = self.make_merged_block(map, new_owner_blocks)?;

        // Is the old neighborhood, minus this block, still valid?
        // TODO refactor Neighborhood to BlockIDs?
        let old_owner_blocks: Vec<BlockID> = self
            .block_to_neighborhood
            .iter()
            .filter_map(|(block, neighborhood)| {
                if *neighborhood == old_owner && *block != id {
                    Some(*block)
                } else {
                    None
                }
            })
            .collect();
        if old_owner_blocks.is_empty() {
            // We're deleting the old neighborhood!
            self.neighborhoods.get_mut(&new_owner).unwrap().0 = new_neighborhood_block;
            self.neighborhoods.remove(&old_owner).unwrap();
            self.block_to_neighborhood.insert(id, new_owner);
            // Tell the caller to recreate this SelectBoundary state, switching to the neighborhood
            // we just donated to, since the old is now gone
            return Ok(Some(new_owner));
        }

        let old_neighborhood_block = self.make_merged_block(map, old_owner_blocks)?;
        // Great! Do the transfer.
        self.neighborhoods.get_mut(&old_owner).unwrap().0 = old_neighborhood_block;
        self.neighborhoods.get_mut(&new_owner).unwrap().0 = new_neighborhood_block;

        self.block_to_neighborhood.insert(id, new_owner);
        Ok(None)
    }

    /// Needs to find an existing neighborhood to take the block, or make a new one
    pub fn remove_block_from_neighborhood(
        &mut self,
        map: &Map,
        id: BlockID,
        old_owner: NeighborhoodID,
    ) -> Result<Option<NeighborhoodID>> {
        // Find all RoadSideIDs in the block matching the current neighborhood perimeter. Look for
        // the first one that borders another neighborhood, and transfer the block there.
        // TODO This can get unintuitive -- if we remove a block bordering two other
        // neighborhoods, which one should we donate to?
        let current_perim_set: BTreeSet<RoadSideID> = self.neighborhoods[&old_owner]
            .0
            .perimeter
            .roads
            .iter()
            .cloned()
            .collect();
        for road_side in &self.get_block(id).perimeter.roads {
            if !current_perim_set.contains(road_side) {
                continue;
            }
            // Is there another neighborhood that has the other side of this road on its perimeter?
            // TODO We could map road -> BlockID then use block_to_neighborhood
            let other_side = road_side.other_side();
            if let Some((new_owner, _)) = self
                .neighborhoods
                .iter()
                .find(|(_, (block, _))| block.perimeter.roads.contains(&other_side))
            {
                let new_owner = *new_owner;
                return self.transfer_block(map, id, old_owner, new_owner);
            }
        }

        // We didn't find any match, so we're jettisoning a block near the edge of the map (or a
        // buggy area missing blocks). Create a new neighborhood with just this block.
        let new_owner = NeighborhoodID(self.neighborhood_id_counter);
        self.neighborhood_id_counter += 1;
        // Temporary color
        self.neighborhoods
            .insert(new_owner, (self.get_block(id).clone(), Color::RED));
        let result = self.transfer_block(map, id, old_owner, new_owner);
        if result.is_err() {
            // Revert the change above!
            self.neighborhoods.remove(&new_owner).unwrap();
        }
        result
    }
}

// Read-only
impl Partitioning {
    pub fn neighborhood_block(&self, id: NeighborhoodID) -> &Block {
        &self.neighborhoods[&id].0
    }

    pub fn neighborhood_color(&self, id: NeighborhoodID) -> Color {
        self.neighborhoods[&id].1
    }

    pub fn all_neighborhoods(&self) -> &BTreeMap<NeighborhoodID, (Block, Color)> {
        &self.neighborhoods
    }

    // Just used for initial creation
    fn neighborhood_containing(&self, find_block: BlockID) -> Option<NeighborhoodID> {
        // TODO We could probably build this mapping up when we do Perimeter::merge_all
        let find_block = self.get_block(find_block);
        for (id, (block, _)) in &self.neighborhoods {
            if block.perimeter.contains(&find_block.perimeter) {
                return Some(*id);
            }
        }
        None
    }

    pub fn all_single_blocks(&self) -> Vec<(BlockID, &Block)> {
        self.single_blocks
            .iter()
            .enumerate()
            .map(|(idx, block)| (BlockID(idx), block))
            .collect()
    }

    pub fn all_block_ids(&self) -> Vec<BlockID> {
        (0..self.single_blocks.len()).map(BlockID).collect()
    }

    pub fn get_block(&self, id: BlockID) -> &Block {
        &self.single_blocks[id.0]
    }

    // Will crash if the original matching failed
    pub fn block_to_neighborhood(&self, id: BlockID) -> NeighborhoodID {
        self.block_to_neighborhood[&id]
    }

    /// Blocks on the "frontier" are adjacent to the perimeter, either just inside or outside.
    pub fn calculate_frontier(&self, perim: &Perimeter) -> BTreeSet<BlockID> {
        let perim_roads: BTreeSet<RoadID> = perim.roads.iter().map(|id| id.road).collect();

        let mut frontier = BTreeSet::new();
        for (block_id, block) in self.all_single_blocks() {
            for road_side_id in &block.perimeter.roads {
                // If the perimeter has this RoadSideID on the same side, we're just inside. If it has
                // the other side, just on the outside. Either way, on the frontier.
                if perim_roads.contains(&road_side_id.road) {
                    frontier.insert(block_id);
                    break;
                }
            }
        }
        frontier
    }

    fn make_merged_block(&self, map: &Map, input: Vec<BlockID>) -> Result<Block> {
        let mut perimeters = Vec::new();
        for id in input {
            perimeters.push(self.get_block(id).perimeter.clone());
        }
        let mut merged = Perimeter::merge_all(perimeters, false);
        if merged.len() != 1 {
            bail!(format!(
                "Splitting this neighborhood into {} pieces is currently unsupported",
                merged.len()
            ));
        }
        merged.pop().unwrap().to_block(map)
    }
}
