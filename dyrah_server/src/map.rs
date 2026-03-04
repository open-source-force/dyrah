use std::collections::HashMap;

use crate::components::{Collider, TilePos};
use dyrah_shared::map::TiledMap;
use glam::{IVec2, Vec2};
use pathfinding::prelude::astar;
use secs::World;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TileOccupant {
    Empty,
    Collider,
    Hole(i16), // fall to layer
}

pub struct TileGrid {
    width: usize,
    height: usize,
    layers: HashMap<i16, Vec<TileOccupant>>, // z -> flat vec
}
impl TileGrid {
    pub fn new(map: &Map) -> Self {
        let width = map.tiled.width as usize;
        let height = map.tiled.height as usize;
        let mut layers = HashMap::new();
        let floors: Vec<i16> = map
            .tiled
            .layers
            .iter()
            .filter_map(|l| {
                l.name
                    .split_once('/')
                    .and_then(|(z, _)| z.parse::<i16>().ok())
            })
            .collect::<std::collections::HashSet<i16>>()
            .into_iter()
            .collect();

        for z in floors {
            let mut grid = vec![TileOccupant::Empty; width * height];

            let colliders_name = format!("{}/colliders", z);
            if let Some(layer) = map.tiled.get_layer(&colliders_name) {
                if let Some(data) = &layer.data {
                    for y in 0..layer.height.unwrap() as usize {
                        for x in 0..layer.width.unwrap() as usize {
                            if data[y * layer.width.unwrap() as usize + x] != 0 {
                                grid[y * width + x] = TileOccupant::Collider;
                            }
                        }
                    }
                }
            }

            let obstacles_name = format!("{}/obstacles", z);
            for o in map.tiled.get_objects(&obstacles_name) {
                if o.name == "hole" {
                    let tile = map.tiled.world_to_tile(Vec2::new(o.x, o.y));
                    let fall_to = o
                        .properties
                        .as_ref()
                        .and_then(|props| {
                            props
                                .iter()
                                .find(|p| p.name == "fall_to_layer")
                                .and_then(|p| p.value.parse::<i16>().ok())
                        })
                        .unwrap_or(z - 1);
                    grid[tile.y as usize * width + tile.x as usize] = TileOccupant::Hole(fall_to);
                }
            }

            layers.insert(z, grid);
        }

        Self {
            width,
            height,
            layers,
        }
    }

    pub fn get(&self, z: i16, tile: IVec2) -> Option<TileOccupant> {
        if tile.x < 0 || tile.y < 0 || tile.x >= self.width as i32 || tile.y >= self.height as i32 {
            return None;
        }
        self.layers
            .get(&z)
            .map(|grid| grid[tile.y as usize * self.width + tile.x as usize])
    }

    pub fn is_walkable(&self, z: i16, tile: IVec2) -> bool {
        match self.get(z, tile) {
            Some(TileOccupant::Empty) | Some(TileOccupant::Hole(_)) => true,
            Some(TileOccupant::Collider) | None => false,
        }
    }

    pub fn update(&mut self, world: &World) {
        // clear only dynamic occupants (keep static colliders/holes)
        // or just re-mark player/creature positions
        world.query(|_, _: &Collider, tile_pos: &TilePos| {
            let z = tile_pos.z;
            if let Some(grid) = self.layers.get_mut(&z) {
                let x = tile_pos.vec.x as usize;
                let y = tile_pos.vec.y as usize;
                if x < self.width && y < self.height {
                    grid[y * self.width + x] = TileOccupant::Collider;
                }
            }
        });
    }
}

pub struct Map {
    pub tiled: TiledMap,
    pub current_z: i16,
}

impl Map {
    pub fn new(path: &str, z: i16) -> Self {
        Self {
            tiled: TiledMap::new(path),
            current_z: z,
        }
    }

    pub fn get_spawn(&self, name: &str) -> Option<IVec2> {
        self.tiled
            .get_object(&format!("{}/spawns", self.current_z), name)
            .map(|o| self.tiled.world_to_tile(Vec2::new(o.x, o.y)))
    }

    pub fn get_spawns(&self) -> Vec<(String, IVec2)> {
        self.tiled
            .get_objects(&format!("{}/spawns", self.current_z))
            .map(|o| {
                (
                    o.name.clone(),
                    self.tiled.world_to_tile(Vec2::new(o.x, o.y)),
                )
            })
            .collect()
    }

    pub fn get_objects_on_floor(&self, floor: i16, layer: &str) -> Vec<(String, IVec2)> {
        self.tiled
            .get_objects(&format!("{}/{}", floor, layer))
            .map(|o| {
                (
                    o.name.clone(),
                    self.tiled.world_to_tile(Vec2::new(o.x, o.y)),
                )
            })
            .collect()
    }

    pub fn is_walkable(&self, tile_pos: IVec2, grid: &TileGrid) -> bool {
        grid.is_walkable(self.current_z, tile_pos)
    }

    fn manhattan_distance(&self, a: IVec2, b: IVec2) -> u32 {
        ((a.x - b.x).abs() + (a.y - b.y).abs()) as u32
    }

    fn get_walkable_successors(&self, tile_pos: IVec2, grid: &TileGrid) -> Vec<(IVec2, u32)> {
        let mut successors = Vec::new();
        for (dx, dy) in &[(0, 1), (1, 0), (0, -1), (-1, 0)] {
            let neighbor = IVec2::new(tile_pos.x + dx, tile_pos.y + dy);
            if neighbor.x >= 0
                && neighbor.y >= 0
                && neighbor.x < self.tiled.width as i32
                && neighbor.y < self.tiled.height as i32
                && self.is_walkable(neighbor, grid)
            {
                successors.push((neighbor, 1));
            }
        }
        successors
    }

    pub fn find_path(&self, start: IVec2, end: IVec2, grid: &TileGrid) -> Option<Vec<IVec2>> {
        astar(
            &start,
            |&pos| self.get_walkable_successors(pos, grid),
            |&pos| self.manhattan_distance(pos, end),
            |&pos| pos == end,
        )
        .map(|(path, _)| path.into_iter().skip(1).collect())
    }

    pub fn is_walkable_at(&self, z: i16, tile_pos: IVec2, grid: &TileGrid) -> bool {
        grid.is_walkable(z, tile_pos)
    }

    pub fn find_path_on(
        &self,
        z: i16,
        start: IVec2,
        end: IVec2,
        grid: &TileGrid,
    ) -> Option<Vec<IVec2>> {
        astar(
            &start,
            |&pos| {
                let mut successors = Vec::new();
                for (dx, dy) in &[(0, 1), (1, 0), (0, -1), (-1, 0)] {
                    let neighbor = IVec2::new(pos.x + dx, pos.y + dy);
                    if neighbor.x >= 0
                        && neighbor.y >= 0
                        && neighbor.x < self.tiled.width as i32
                        && neighbor.y < self.tiled.height as i32
                        && grid.is_walkable(z, neighbor)
                    {
                        successors.push((neighbor, 1));
                    }
                }
                successors
            },
            |&pos| self.manhattan_distance(pos, end),
            |&pos| pos == end,
        )
        .map(|(path, _)| path.into_iter().skip(1).collect())
    }
}
