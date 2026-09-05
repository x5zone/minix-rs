//! Dungeon rooms and corridors.
//!
//! Ground truth: `minix3/games/rogue/rogue.h` (wall and door flags near lines
//! 54 to 56, at most `MAXROOMS 9` rooms near line 293, passage marker near
//! line 298) and `room.c` (room placement). A level holds at most nine rooms;
//! rooms must not overlap; corridors carve right angle paths between room
//! doors. Monsters, objects, and scoring stay with later work; this module
//! owns the map geometry.

use crate::TermGameError;

/// Largest room count per level.
pub const MAX_ROOMS: usize = 9;

/// Map width and height in cells.
pub const MAP_WIDTH: i32 = 80;
/// Map width and height in cells.
pub const MAP_HEIGHT: i32 = 24;

/// Map cell kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DungeonCell {
    /// Unexplored rock.
    #[default]
    Rock,
    /// Room floor.
    Floor,
    /// Horizontal or vertical wall.
    Wall,
    /// Doorway between room and corridor.
    Door,
    /// Corridor floor.
    Passage,
}

/// One room (top left corner plus size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    /// Left column.
    pub x: i32,
    /// Top row.
    pub y: i32,
    /// Width in cells.
    pub width: i32,
    /// Height in cells.
    pub height: i32,
}

/// Check one room (positive size, inside the map, with a wall margin).
pub fn check_room(room: Room) -> Result<(), TermGameError> {
    if room.width < 3 || room.height < 3 {
        return Err(TermGameError::InvalidArgument);
    }
    if room.x < 1
        || room.y < 1
        || room.x + room.width > MAP_WIDTH - 1
        || room.y + room.height > MAP_HEIGHT - 1
    {
        return Err(TermGameError::InvalidArgument);
    }
    Ok(())
}

/// True when two rooms overlap (one cell of wall margin counts as overlap,
/// so neighbors never share walls).
pub fn rooms_overlap(first: Room, second: Room) -> bool {
    first.x - 1 < second.x + second.width
        && second.x - 1 < first.x + first.width
        && first.y - 1 < second.y + second.height
        && second.y - 1 < first.y + first.height
}

/// Validate a room list (each room valid, no two overlapping, at most nine).
pub fn check_room_list(rooms: &[Room]) -> Result<(), TermGameError> {
    if rooms.len() > MAX_ROOMS {
        return Err(TermGameError::InvalidArgument);
    }
    for room in rooms {
        check_room(*room)?;
    }
    for (index, first) in rooms.iter().enumerate() {
        for second in &rooms[index + 1..] {
            if rooms_overlap(*first, *second) {
                return Err(TermGameError::InvalidArgument);
            }
        }
    }
    Ok(())
}

/// Carve a right angle corridor from (`x1`, `y1`) to (`x2`, `y2`): first
/// horizontally, then vertically. Rock becomes passage; everything else
/// stays (doors and floors are never overwritten).
pub fn carve_corridor(
    map: &mut [DungeonCell],
    width: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) -> Result<(), TermGameError> {
    if width == 0 || !map.len().is_multiple_of(width) {
        return Err(TermGameError::InvalidArgument);
    }
    let height = map.len() / width;
    let mut carve = |x: i32, y: i32| -> Result<(), TermGameError> {
        if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
            return Err(TermGameError::InvalidArgument);
        }
        let index = y as usize * width + x as usize;
        if map[index] == DungeonCell::Rock {
            map[index] = DungeonCell::Passage;
        }
        Ok(())
    };
    let mut x = x1;
    while x != x2 {
        carve(x, y1)?;
        x += if x2 > x { 1 } else { -1 };
    }
    let mut y = y1;
    while y != y2 {
        carve(x2, y)?;
        y += if y2 > y { 1 } else { -1 };
    }
    carve(x2, y2)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(x: i32, y: i32) -> Room {
        Room { x, y, width: 6, height: 4 }
    }

    #[test]
    fn test_rooms_checked() {
        assert_eq!(check_room(room(2, 2)), Ok(()));
        assert_eq!(
            check_room(Room { x: 0, y: 2, width: 6, height: 4 }),
            Err(TermGameError::InvalidArgument)
        );
        assert_eq!(
            check_room(Room { x: 2, y: 2, width: 2, height: 4 }),
            Err(TermGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_overlap_detected() {
        assert!(rooms_overlap(room(2, 2), room(5, 3)));
        assert!(!rooms_overlap(room(2, 2), room(20, 10)));
    }

    #[test]
    fn test_list_validated() {
        assert_eq!(check_room_list(&[room(2, 2), room(20, 10)]), Ok(()));
        assert_eq!(
            check_room_list(&[room(2, 2), room(5, 3)]),
            Err(TermGameError::InvalidArgument)
        );
        let many = [room(2, 2); 10];
        assert_eq!(check_room_list(&many), Err(TermGameError::InvalidArgument));
    }

    #[test]
    fn test_corridor_carves() {
        let mut map = [DungeonCell::Rock; 20 * 10];
        carve_corridor(&mut map, 20, 2, 2, 6, 5).unwrap();
        assert_eq!(map[2 * 20 + 2], DungeonCell::Passage);
        assert_eq!(map[5 * 20 + 6], DungeonCell::Passage);
        // Floors survive carving.
        map[3 * 20 + 4] = DungeonCell::Floor;
        carve_corridor(&mut map, 20, 2, 3, 6, 3).unwrap();
        assert_eq!(map[3 * 20 + 4], DungeonCell::Floor);
    }

    #[test]
    fn test_bad_maps_rejected() {
        let mut map = [DungeonCell::Rock; 10];
        assert_eq!(
            carve_corridor(&mut map, 3, 0, 0, 2, 2),
            Err(TermGameError::InvalidArgument)
        );
    }
}
