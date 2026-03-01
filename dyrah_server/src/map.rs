use crate::components::{Collider, TilePos};
use dyrah_shared::map::TiledMap;
use glam::{IVec2, Vec2};
use pathfinding::prelude::astar;
use secs::World;

pub struct CollisionGrid {
    width: usize,
    height: usize,
    grid: Vec<bool>,
}

impl CollisionGrid {
    pub fn new(map: &Map) -> Self {
        let width = map.tiled.width as usize;
        let height = map.tiled.height as usize;
        Self {
            width,
            height,
            grid: vec![false; width * height],
        }
    }

    pub fn update(&mut self, map: &Map, world: &World) {
        self.grid.fill(false);
        for y in 0..self.height {
            for x in 0..self.width {
                let tile_pos = IVec2::new(x as i32, y as i32);
                if !map.tiled.is_walkable("level_0/colliders", tile_pos) {
                    self.grid[y * self.width + x] = true;
                }
            }
        }
        world.query(|_, _: &Collider, tile_pos: &TilePos| {
            let x = tile_pos.vec.x as usize;
            let y = tile_pos.vec.y as usize;
            if x < self.width && y < self.height {
                self.grid[y * self.width + x] = true;
            }
        });
    }

    pub fn is_walkable(&self, tile_pos: IVec2) -> bool {
        let (x, y) = (tile_pos.x as usize, tile_pos.y as usize);
        if x >= self.width || y >= self.height {
            false
        } else {
            !self.grid[y * self.width + x]
        }
    }
}

pub struct Map {
    pub tiled: TiledMap,
}

impl Map {
    pub fn new(path: &str) -> Self {
        Self {
            tiled: TiledMap::new(path),
        }
    }

    pub fn get_spawn(&self, name: &str) -> Option<IVec2> {
        self.tiled
            .get_object("level_0/spawns", name)
            .map(|o| self.tiled.world_to_tile(Vec2::new(o.x, o.y)))
    }

    pub fn is_walkable(&self, tile_pos: IVec2, grid: &CollisionGrid) -> bool {
        grid.is_walkable(tile_pos)
    }

    fn chebyshev_distance(&self, a: IVec2, b: IVec2) -> u32 {
        let dx = (a.x - b.x).abs() as u32;
        let dy = (a.y - b.y).abs() as u32;
        // cardinal cost=2, diagonal cost=3; Chebyshev: max(dx,dy)*2 + min(dx,dy)*(3-2)
        let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
        max * 2 + min
    }

    fn get_walkable_successors(&self, tile_pos: IVec2, grid: &CollisionGrid) -> Vec<(IVec2, u32)> {
        let mut successors = Vec::new();
        for &(dx, dy, cost) in &[
            (0, 1, 2),
            (1, 0, 2),
            (0, -1, 2),
            (-1, 0, 2),
            (1, 1, 3),
            (1, -1, 3),
            (-1, 1, 3),
            (-1, -1, 3),
        ] {
            let neighbor = IVec2::new(tile_pos.x + dx, tile_pos.y + dy);
            if neighbor.x >= 0
                && neighbor.y >= 0
                && neighbor.x < self.tiled.width as i32
                && neighbor.y < self.tiled.height as i32
                && grid.is_walkable(neighbor)
            {
                // for diagonals, also require both adjacent cardinal tiles to be walkable
                if dx != 0 && dy != 0 {
                    let adj_x = IVec2::new(tile_pos.x + dx, tile_pos.y);
                    let adj_y = IVec2::new(tile_pos.x, tile_pos.y + dy);
                    if !grid.is_walkable(adj_x) || !grid.is_walkable(adj_y) {
                        continue;
                    }
                }
                successors.push((neighbor, cost));
            }
        }
        successors
    }

    pub fn find_path(&self, start: IVec2, end: IVec2, grid: &CollisionGrid) -> Option<Vec<IVec2>> {
        let result = astar(
            &start,
            |&pos| self.get_walkable_successors(pos, grid),
            |&pos| self.chebyshev_distance(pos, end),
            |&pos| pos == end,
        );
        result.map(|(path, _)| path.into_iter().skip(1).collect())
    }
}
