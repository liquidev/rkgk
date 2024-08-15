use super::ChunkPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkIterator {
    cursor: ChunkPosition,
    left: i32,
    bottom_right: ChunkPosition,
}

impl ChunkIterator {
    pub fn new(top_left: ChunkPosition, bottom_right: ChunkPosition) -> Self {
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
