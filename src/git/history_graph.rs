//! Incremental lane layout for a child-before-parent commit stream.
//! Retain this state between pages; edges ending at the page boundary continue
//! into the next page instead of inventing disconnected roots.
#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub color: usize,
    pub from: usize,
    pub to: usize,
    pub start: f32,
    pub end: f32,
}
#[derive(Clone, Debug)]
pub struct GraphRow {
    pub color: usize,
    pub lane: usize,
    pub width: usize,
    pub edges: Vec<GraphEdge>,
}
#[derive(Default)]
pub struct HistoryGraph {
    lanes: Vec<(String, usize)>,
    next_color: usize,
}
impl HistoryGraph {
    fn allocate_color(&mut self) -> usize {
        let color = self.next_color;
        self.next_color += 1;
        color
    }
    pub fn append(&mut self, id: &str, parents: &[String]) -> GraphRow {
        let existing = self.lanes.iter().position(|(p, _)| p == id);
        let lane = existing.unwrap_or(self.lanes.len());
        if existing.is_none() {
            let color = self.allocate_color();
            self.lanes.push((id.to_owned(), color));
        }
        let color = self.lanes[lane].1;
        let before = self.lanes.clone();
        self.lanes.remove(lane);
        for (index, parent) in parents.iter().enumerate() {
            if !self.lanes.iter().any(|(id, _)| id == parent) {
                let parent_color = if index == 0 {
                    color
                } else {
                    self.allocate_color()
                };
                self.lanes.insert(
                    (lane + index).min(self.lanes.len()),
                    (parent.clone(), parent_color),
                );
            }
        }
        let mut edges = Vec::new();
        if existing.is_some() {
            edges.push(GraphEdge {
                color,
                from: lane,
                to: lane,
                start: 0.,
                end: 0.5,
            });
        }
        for (from, target) in before.iter().enumerate() {
            if from == lane {
                continue;
            }
            let to = self
                .lanes
                .iter()
                .position(|p| p == target)
                .expect("continuing graph lane");
            edges.push(GraphEdge {
                color: target.1,
                from,
                to: from,
                start: 0.,
                end: 0.5,
            });
            edges.push(GraphEdge {
                color: target.1,
                from,
                to,
                start: 0.5,
                end: 1.,
            });
        }
        for (index, parent) in parents.iter().enumerate() {
            let to = self
                .lanes
                .iter()
                .position(|(id, _)| id == parent)
                .expect("parent graph lane");
            edges.push(GraphEdge {
                color: if index == 0 { color } else { self.lanes[to].1 },
                from: lane,
                to,
                start: 0.5,
                end: 1.,
            });
        }
        GraphRow {
            color,
            lane,
            width: before.len().max(self.lanes.len()),
            edges,
        }
    }
}
