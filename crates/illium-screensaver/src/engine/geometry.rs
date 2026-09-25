//! Port of `terminaltexteffects.utils.geometry` (release 0.15.0).
//! Rows grow upwards: row 1 is the bottom of the canvas.

/// Python's `round()`: ties go to the even integer.
pub fn round(x: f64) -> i32 {
    x.round_ties_even() as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Coord {
    pub column: i32,
    pub row: i32,
}

impl Coord {
    pub const fn new(column: i32, row: i32) -> Self {
        Self { column, row }
    }
}

pub fn find_coords_on_circle(
    origin: Coord,
    radius: i32,
    coords_limit: usize,
    unique: bool,
) -> Vec<Coord> {
    let mut points = Vec::new();
    if radius == 0 {
        return points;
    }
    let limit = if coords_limit == 0 {
        round(2.0 * std::f64::consts::PI * radius as f64).max(1) as usize
    } else {
        coords_limit
    };
    let step = 2.0 * std::f64::consts::PI / limit as f64;
    let mut seen = std::collections::HashSet::new();
    for i in 0..limit {
        let angle = step * i as f64;
        let mut x = origin.column as f64 + radius as f64 * angle.cos();
        x += x - origin.column as f64;
        let y = origin.row as f64 + radius as f64 * angle.sin();
        let point = Coord::new(round(x), round(y));
        if !unique || !seen.contains(&point) {
            points.push(point);
        }
        seen.insert(point);
    }
    points
}

pub fn find_coords_in_circle(center: Coord, diameter: i32) -> Vec<Coord> {
    let mut coords = Vec::new();
    if diameter == 0 {
        return coords;
    }
    let a_squared = (diameter as f64).powi(2);
    let b_squared = (diameter as f64 / 2.0).powi(2);
    for x in center.column - diameter..=center.column + diameter {
        let x_component = ((x - center.column) as f64).powi(2) / a_squared;
        let max_y_offset = (b_squared * (1.0 - x_component)).sqrt() as i32;
        for y in center.row - max_y_offset..=center.row + max_y_offset {
            coords.push(Coord::new(x, y));
        }
    }
    coords
}

pub fn find_coords_in_rect(origin: Coord, distance: i32) -> Vec<Coord> {
    let mut coords = Vec::new();
    if distance == 0 {
        return coords;
    }
    for column in origin.column - distance..=origin.column + distance {
        for row in origin.row - distance..=origin.row + distance {
            coords.push(Coord::new(column, row));
        }
    }
    coords
}

pub fn find_coords_on_rect(origin: Coord, half_width: i32, half_height: i32) -> Vec<Coord> {
    let mut coords = Vec::new();
    if half_width == 0 || half_height == 0 {
        return coords;
    }
    for column in origin.column - half_width..=origin.column + half_width {
        if column == origin.column - half_width || column == origin.column + half_width {
            for row in origin.row - half_height..=origin.row + half_height {
                coords.push(Coord::new(column, row));
            }
        } else {
            coords.push(Coord::new(column, origin.row - half_height));
            coords.push(Coord::new(column, origin.row + half_height));
        }
    }
    coords
}

pub fn extrapolate_along_ray(origin: Coord, target: Coord, offset_from_target: f64) -> Coord {
    let total_distance = find_length_of_line(origin, target, false) + offset_from_target;
    if total_distance == 0.0 || origin == target {
        return target;
    }
    let t = total_distance / find_length_of_line(origin, target, false);
    let column = (1.0 - t) * origin.column as f64 + t * target.column as f64;
    let row = (1.0 - t) * origin.row as f64 + t * target.row as f64;
    Coord::new(round(column), round(row))
}

pub fn find_coord_on_bezier_curve(start: Coord, control: &[Coord], end: Coord, t: f64) -> Coord {
    let mut points: Vec<(f64, f64)> = std::iter::once(start)
        .chain(control.iter().copied())
        .chain(std::iter::once(end))
        .map(|c| (c.column as f64, c.row as f64))
        .collect();
    while points.len() > 1 {
        points = points
            .windows(2)
            .map(|p| {
                (
                    (1.0 - t) * p[0].0 + t * p[1].0,
                    (1.0 - t) * p[0].1 + t * p[1].1,
                )
            })
            .collect();
    }
    Coord::new(round(points[0].0), round(points[0].1))
}

pub fn find_coord_on_line(start: Coord, end: Coord, t: f64) -> Coord {
    let x = (1.0 - t) * start.column as f64 + t * end.column as f64;
    let y = (1.0 - t) * start.row as f64 + t * end.row as f64;
    Coord::new(round(x), round(y))
}

pub fn find_length_of_bezier_curve(start: Coord, control: &[Coord], end: Coord) -> f64 {
    let mut length = 0.0;
    let mut previous = start;
    for t in 1..10 {
        let coord = find_coord_on_bezier_curve(start, control, end, t as f64 / 10.0);
        length += find_length_of_line(previous, coord, true);
        previous = coord;
    }
    length
}

pub fn find_length_of_line(a: Coord, b: Coord, double_row_diff: bool) -> f64 {
    let column_diff = (b.column - a.column) as f64;
    let row_diff = (b.row - a.row) as f64;
    if double_row_diff {
        column_diff.hypot(2.0 * row_diff)
    } else {
        column_diff.hypot(row_diff)
    }
}

pub fn find_normalized_distance_from_center(
    bottom: i32,
    top: i32,
    left: i32,
    right: i32,
    other: Coord,
) -> f64 {
    let y_offset = bottom - 1;
    let x_offset = left - 1;
    let right = (right - x_offset) as f64;
    let top = (top - y_offset) as f64;
    let center_x = right / 2.0;
    let center_y = top / 2.0;
    let max_distance = (right.powi(2) + (top * 2.0).powi(2)).sqrt();
    let distance = (((other.column - x_offset) as f64 - center_x).powi(2)
        + (((other.row - y_offset) as f64 - center_y) * 2.0).powi(2))
    .sqrt();
    distance / (max_distance / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_rounding_and_lines() {
        assert_eq!(round(2.5), 2);
        assert_eq!(round(3.5), 4);
        assert_eq!(round(-0.5), 0);
        assert_eq!(
            find_coord_on_line(Coord::new(1, 1), Coord::new(4, 1), 0.5),
            Coord::new(2, 1)
        );
        assert_eq!(
            find_length_of_line(Coord::new(0, 0), Coord::new(3, 2), true),
            5.0
        );
    }
    #[test]
    fn circles_double_columns() {
        let circle = find_coords_on_circle(Coord::new(10, 10), 2, 4, true);
        assert_eq!(
            circle,
            [
                Coord::new(14, 10),
                Coord::new(10, 12),
                Coord::new(6, 10),
                Coord::new(10, 8)
            ]
        );
        assert!(find_coords_in_circle(Coord::new(0, 0), 0).is_empty());
    }
}
