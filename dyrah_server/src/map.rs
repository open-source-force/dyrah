use std::collections::HashMap;

use crate::components::{Collider, TilePos};
use dyrah_shared::map::TiledMap;
use glam::{IVec2, Vec2};
use pathfinding::prelude::astar;
use secs::World;

pub struct CollisionGrid {
    width: usize,
    height: usize,
    grids: HashMap<i16, Vec<bool>>, // cached per-layer
}

impl CollisionGrid {
    pub fn new(map: &Map) -> Self {
        let width = map.tiled.width as usize;
        let height = map.tiled.height as usize;
        Self {
            width,
            height,
            grids: HashMap::new(),
        }
    }

    pub fn update(&mut self, map: &Map, world: &World) {
        let layer = map.current_z;
        let mut grid = vec![false; self.width * self.height];

        // map layer colliders
        for y in 0..self.height {
            for x in 0..self.width {
                let tile_pos = IVec2::new(x as i32, y as i32);
                if !map
                    .tiled
                    .is_walkable(&format!("{}/colliders", layer), tile_pos)
                {
                    grid[y * self.width + x] = true;
                }
            }
        }

        // world colliders
        world.query(|_, _: &Collider, tile_pos: &TilePos| {
            let x = tile_pos.vec.x as usize;
            let y = tile_pos.vec.y as usize;
            if x < self.width && y < self.height {
                grid[y * self.width + x] = true;
            }
        });

        self.grids.insert(layer.clone(), grid);
    }

    pub fn is_walkable(&self, z: i16, tile_pos: IVec2) -> bool {
        let (x, y) = (tile_pos.x as usize, tile_pos.y as usize);
        if x >= self.width || y >= self.height {
            return false;
        }

        self.grids
            .get(&z)
            .map(|grid| !grid[y * self.width + x])
            .unwrap_or(true) // assume walkable if grid missing
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

    // spawn helpers
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

    // layer-aware walkability
    pub fn is_walkable(&self, tile_pos: IVec2, grid: &CollisionGrid) -> bool {
        grid.is_walkable(self.current_z, tile_pos)
    }

    // pathfinding helpers
    fn manhattan_distance(&self, a: IVec2, b: IVec2) -> u32 {
        ((a.x - b.x).abs() + (a.y - b.y).abs()) as u32
    }

    fn get_walkable_successors(&self, tile_pos: IVec2, grid: &CollisionGrid) -> Vec<(IVec2, u32)> {
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

    pub fn find_path(&self, start: IVec2, end: IVec2, grid: &CollisionGrid) -> Option<Vec<IVec2>> {
        astar(
            &start,
            |&pos| self.get_walkable_successors(pos, grid),
            |&pos| self.manhattan_distance(pos, end),
            |&pos| pos == end,
        )
        .map(|(path, _)| path.into_iter().skip(1).collect())
    }
}
