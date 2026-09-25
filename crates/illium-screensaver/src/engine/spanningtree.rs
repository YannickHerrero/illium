//! Port of `terminaltexteffects.utils.spanningtree` (release 0.15.0): the
//! generators the effects use. `EffectCharacter.links` lives in [`Links`],
//! owned by the generator that builds the tree.
use super::character::CharId;
use super::geometry::Coord;
use super::terminal::Terminal;
use std::collections::{BTreeMap, BTreeSet};

/// `links` of every character, indexed by id.
#[derive(Clone, Debug, Default)]
pub struct Links(Vec<BTreeSet<CharId>>);

impl Links {
    fn new(t: &Terminal) -> Self {
        Self(vec![BTreeSet::new(); t.chars.len()])
    }
    /// `EffectCharacter._link` (bidirectional).
    fn link(&mut self, a: CharId, b: CharId) {
        self.0[b].insert(a);
        self.0[a].insert(b);
    }
    fn of(&self, id: CharId) -> &BTreeSet<CharId> {
        &self.0[id]
    }
    fn is_linked(&self, id: CharId) -> bool {
        !self.0[id].is_empty()
    }
}

/// `SpanningTreeGenerator.get_neighbors` with `unlinked_only=True`.
/// Neighbors come in `EffectCharacter.neighbors` order: north, east, south, west.
fn unlinked_neighbors(t: &Terminal, links: &Links, id: CharId, limit: bool) -> Vec<CharId> {
    let at = t.chars[id].input_coord;
    [(0, 1), (1, 0), (0, -1), (-1, 0)]
        .into_iter()
        .filter_map(|(dc, dr)| {
            t.get_character_by_input_coord(Coord::new(at.column + dc, at.row + dr))
        })
        .filter(|&n| !limit || t.canvas.coord_is_in_text(t.chars[n].input_coord))
        .filter(|&n| !links.is_linked(n))
        .collect()
}

fn starting_char(t: &mut Terminal, limit: bool) -> CharId {
    let coord = t.canvas.random_coord(&mut t.rng, false, limit);
    t.get_character_by_input_coord(coord)
        .expect("Unable to find a starting character.")
}

/// `PrimsSimple`: equal weights, random edge each step.
pub struct PrimsSimple {
    limit_to_text_boundary: bool,
    pub links: Links,
    pub char_link_order: Vec<CharId>,
    edge_chars: Vec<CharId>,
    pub complete: bool,
}

impl PrimsSimple {
    pub fn new(t: &mut Terminal, start: Option<CharId>, limit_to_text_boundary: bool) -> Self {
        let start = start.unwrap_or_else(|| starting_char(t, limit_to_text_boundary));
        Self {
            limit_to_text_boundary,
            links: Links::new(t),
            char_link_order: vec![start],
            edge_chars: vec![start],
            complete: false,
        }
    }
    pub fn step(&mut self, t: &mut Terminal) {
        if self.edge_chars.is_empty() {
            self.complete = true;
            return;
        }
        let i = t.rng.randrange(0, self.edge_chars.len() as i64) as usize;
        let current = self.edge_chars.remove(i);
        let limit = self.limit_to_text_boundary;
        let mut unlinked = unlinked_neighbors(t, &self.links, current, limit);
        if unlinked.is_empty() {
            return;
        }
        let j = t.rng.randrange(0, unlinked.len() as i64) as usize;
        let next = unlinked.remove(j);
        self.links.link(current, next);
        self.char_link_order.push(next);
        if !unlinked.is_empty() {
            self.edge_chars.push(current);
        }
        if !unlinked_neighbors(t, &self.links, next, limit).is_empty() {
            self.edge_chars.push(next);
        }
    }
}

/// `RecursiveBacktracker`: depth first with an explicit stack.
pub struct RecursiveBacktracker {
    limit_to_text_boundary: bool,
    current: CharId,
    pub links: Links,
    pub char_link_order: Vec<CharId>,
    stack: Vec<CharId>,
    pub complete: bool,
}

impl RecursiveBacktracker {
    pub fn new(t: &mut Terminal, start: Option<CharId>, limit_to_text_boundary: bool) -> Self {
        let start = start.unwrap_or_else(|| starting_char(t, limit_to_text_boundary));
        Self {
            limit_to_text_boundary,
            current: start,
            links: Links::new(t),
            char_link_order: vec![start],
            stack: vec![start],
            complete: false,
        }
    }
    pub fn step(&mut self, t: &mut Terminal) {
        if self.stack.is_empty() {
            self.complete = true;
            return;
        }
        let unvisited =
            unlinked_neighbors(t, &self.links, self.current, self.limit_to_text_boundary);
        if unvisited.is_empty() {
            self.stack.pop();
            if let Some(&top) = self.stack.last() {
                self.current = top;
            }
        } else {
            let next = *t.rng.choice(&unvisited);
            self.links.link(self.current, next);
            self.char_link_order.push(next);
            self.stack.push(next);
            self.current = next;
        }
    }
}

/// `PrimsWeighted`: links the pending neighbor with the lowest random weight.
pub struct PrimsWeighted {
    limit_to_text_boundary: bool,
    weights: Vec<i64>,
    pub links: Links,
    pub char_link_order: Vec<CharId>,
    /// weight -> [(char_a, char_b)]
    pending: BTreeMap<i64, Vec<(CharId, CharId)>>,
    pub complete: bool,
}

impl PrimsWeighted {
    pub fn new(t: &mut Terminal, start: Option<CharId>, limit_to_text_boundary: bool) -> Self {
        use super::terminal::{CharacterSort, Select};
        let start = start.unwrap_or_else(|| starting_char(t, limit_to_text_boundary));
        let mut weights = vec![0; t.chars.len()];
        for id in t.get_characters(Select::ALL_CELLS, CharacterSort::TopToBottomLeftToRight) {
            weights[id] = t.rng.randint(0, 99);
        }
        let mut effect = Self {
            limit_to_text_boundary,
            weights,
            links: Links::new(t),
            char_link_order: vec![start],
            pending: BTreeMap::new(),
            complete: false,
        };
        effect.add_weighted_links(t, start);
        effect
    }
    fn add_weighted_links(&mut self, t: &Terminal, id: CharId) {
        for neighbor in unlinked_neighbors(t, &self.links, id, self.limit_to_text_boundary) {
            self.pending
                .entry(self.weights[neighbor])
                .or_default()
                .push((id, neighbor));
        }
    }
    fn get_lowest_weight_link(&mut self, t: &mut Terminal) -> Option<(CharId, CharId)> {
        while let Some(mut entry) = self.pending.first_entry() {
            let links = entry.get_mut();
            let i = t.rng.randrange(0, links.len() as i64) as usize;
            let link = links.remove(i);
            if links.is_empty() {
                entry.remove();
            }
            if !self.links.is_linked(link.1) {
                return Some(link);
            }
        }
        None
    }
    pub fn step(&mut self, t: &mut Terminal) {
        if self.pending.is_empty() {
            self.complete = true;
            return;
        }
        let Some((a, b)) = self.get_lowest_weight_link(t) else {
            self.complete = true;
            return;
        };
        self.links.link(a, b);
        self.char_link_order.push(b);
        self.add_weighted_links(t, b);
    }
}

/// `BreadthFirst`: walks an already linked graph one layer per step.
pub struct BreadthFirst {
    links: Links,
    pub starting_char: CharId,
    frontier: Vec<CharId>,
    explored: BTreeSet<CharId>,
    pub explored_last_step: Vec<CharId>,
    pub char_explore_order: Vec<CharId>,
    pub complete: bool,
}

impl BreadthFirst {
    pub fn new(
        t: &mut Terminal,
        links: Links,
        start: Option<CharId>,
        limit_to_text_boundary: bool,
    ) -> Self {
        let start = start.unwrap_or_else(|| starting_char(t, limit_to_text_boundary));
        Self {
            links,
            starting_char: start,
            frontier: vec![start],
            explored: BTreeSet::from([start]),
            explored_last_step: vec![],
            char_explore_order: vec![],
            complete: false,
        }
    }
    pub fn step(&mut self) {
        self.explored_last_step.clear();
        if self.frontier.is_empty() {
            self.complete = true;
            return;
        }
        let mut new_edges: Vec<CharId> = vec![];
        for position in std::mem::take(&mut self.frontier) {
            for &neighbor in self.links.of(position) {
                if self.explored.insert(neighbor) {
                    new_edges.push(neighbor);
                    self.explored_last_step.push(neighbor);
                    self.char_explore_order.push(neighbor);
                }
            }
        }
        self.frontier = new_edges;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sorted(mut ids: Vec<CharId>) -> Vec<CharId> {
        ids.sort();
        ids
    }
    #[test]
    fn generators_span_the_text_boundary() {
        let mut t = Terminal::new("abc\nd f\nghi", 9, 5, 3);
        let in_text: Vec<CharId> = (0..t.chars.len())
            .filter(|&id| t.canvas.coord_is_in_text(t.chars[id].input_coord))
            .collect();
        let mut simple = PrimsSimple::new(&mut t, None, true);
        while !simple.complete {
            simple.step(&mut t);
        }
        assert_eq!(sorted(simple.char_link_order), in_text);
        let mut backtracker = RecursiveBacktracker::new(&mut t, None, true);
        while !backtracker.complete {
            backtracker.step(&mut t);
        }
        assert_eq!(sorted(backtracker.char_link_order), in_text);
        let mut weighted = PrimsWeighted::new(&mut t, None, true);
        while !weighted.complete {
            weighted.step(&mut t);
        }
        assert_eq!(sorted(weighted.char_link_order.clone()), in_text);
        let mut fill = BreadthFirst::new(&mut t, weighted.links, None, true);
        while !fill.complete {
            fill.step();
        }
        let mut explored = fill.char_explore_order;
        explored.push(fill.starting_char);
        assert_eq!(sorted(explored), in_text);
    }
}
