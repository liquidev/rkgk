use super::ChunkPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkIterator {
    cursor: ChunkPosition,
    left: i32,
    bottom_right: ChunkPosition,
}

impl ChunkIterator {
    pub fn new(start: ChunkPosition, end: ChunkPosition) -> Self {
        let top_left = ChunkPosition::new(start.x.min(end.x), start.y.min(end.y));
        let bottom_right = ChunkPosition::new(start.x.max(end.x), start.y.max(end.y));
        Self {
            cursor: top_left,
            left: top_left.x,
            bottom_right,
        }
    }
}

impl Iterator for ChunkIterator {
    type Item = ChunkPosition;

    fn next(&mut self) -> Option<Self::Item> {
        let position = self.cursor;

        self.cursor.x += 1;
        if self.cursor.y > self.bottom_right.y {
            return None;
        }
        if self.cursor.x > self.bottom_right.x {
            self.cursor.x = self.left;
            self.cursor.y += 1;
        }

        Some(position)
    }
}
