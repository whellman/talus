//! Algorithms for analyzing the behavior of a scalar function over a graph.
use petgraph::graph::{UnGraph, NodeIndex, EdgeIndex};
use petgraph::unionfind::UnionFind;

use std::collections::{HashSet, HashMap};
use std::hash::{Hash, Hasher};
use std::cmp::Ordering;
use std::f64;

use super::LabeledPoint;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MorseError {
    #[error("Node {node:?} had NaN for its value")]
    NanValue {node: NodeIndex},

    #[error("Expected node {node:?} in graph but could not find it")]
    MissingNode {node: NodeIndex},

    #[error("Node {node:?} had no neighbors but neighbors were expected")]
    MissingNeighbors {node: NodeIndex},

    #[error("Could not compute gradient, edge {edge:?} had no weight")]
    MissingEdgeWeight {edge: EdgeIndex},

    #[error("Expected edge between {node:?} and {other:?} but could not find it")]
    MissingEdge {node: NodeIndex, other: NodeIndex},

    #[error("Could not find a maximum for node {node:?}")]
    NoMaximum {node: NodeIndex},

    #[error("Could not find data for node {node:?}")]
    MissingData {node: NodeIndex}
}

#[derive(Debug)]
struct MorseData {
    lifetime: f64,
    merge_parent: Option<NodeIndex>,
    ancestor: NodeIndex  // TODO: I dunno what the "proper" name for this is
}

#[derive(Debug)]
struct MorseNode {
    node: NodeIndex,
    data: Option<MorseData>
}

impl MorseNode {
    fn new(node: NodeIndex) -> MorseNode {
        MorseNode{node, data: None}
    }
}

impl Hash for MorseNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}

impl PartialEq for MorseNode {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

impl Eq for MorseNode {}

#[derive(Debug)]
struct PointedUnionFind {
    unionfind: UnionFind<usize>,
    reprs: Vec<usize>
}

impl PointedUnionFind {
    // this is insanely specific and will break if you use it outside of exactly
    // how it works in the morse complex code (and maybe even if you use it
    // exactly that way!)
    // This turns UnionFind into a structure that always keeps the representative
    // for the left hand size of a union constant. But to do this O(1)
    // i can't do things like "ensure consistency" outside of the access patterns
    // i know the morse complex code will follow
    // (specifically, this data structure offers no guarantees that
    // `find(find(x)) will be reasonable)
    fn new(n: usize) -> Self {
        let unionfind = UnionFind::new(n);
        let reprs = (0..n).collect();
        PointedUnionFind{unionfind, reprs}
    }

    fn find(&self, x: usize) -> usize {
        let inner_repr = self.unionfind.find(x);
        self.reprs[inner_repr]
    }

    fn union(&mut self, x: usize, y: usize) {
        // x is privileged!
        let old_outer = self.find(x);
        self.unionfind.union(x, y);
        let new_inner = self.unionfind.find(x);
        self.reprs[new_inner] = old_outer;
    }
}

/// Contains all of the filtration information for a MorseComplex
///
/// A Morse complex, especially one generated from discrete points of empirical data,
/// may contain extrema that are considered spurious. The filtration of a MorseComplex
/// provides a series of simplifications of that complex, created by merging less 
/// persistent extrema with more persistence extrema. Taken to its conclusion, all
/// extrema will have been merged with the global extreme.
///
/// The MorseFiltrationStep struct contains the information corresponding to one
/// step of this simplification process.
#[derive(Debug, Clone, Copy)]
pub struct MorseFiltrationStep {
    pub time: f64,
    pub destroyed_cell: NodeIndex,
    pub owning_cell: NodeIndex
}

/// Indicates whether a MorseComplex is Ascending or Descending.
///
/// See [MorseComplex](struct.MorseComplex.html) for a detailed explanation.
#[derive(Debug, Clone, Copy)]
pub enum MorseKind {
    Ascending,
    Descending
}

/// Contains both the ascending and descending morse complexes constructed
/// from a graph.
///
/// See [MorseComplex](struct.MorseComplex.html) for a detailed explanation.
#[derive(Debug)]
pub struct MorseSmaleComplex {
    pub ascending_complex: MorseComplex,
    pub descending_complex: MorseComplex
}

impl MorseSmaleComplex {
    /// Constructs a MorseSmaleComplex from the given graph.
    pub fn from_graph<T>(graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<MorseSmaleComplex, MorseError> {
        let ascending_complex = MorseComplex::from_graph(MorseKind::Ascending, &graph)?;
        let descending_complex = MorseComplex::from_graph(MorseKind::Descending, &graph)?;

        Ok(MorseSmaleComplex{ascending_complex, descending_complex})
    }
}

/// The Morse complex constructed from a graph.
///
/// A Morse complex is, functionally, a partition of a graph into regions
/// belongs to the various extrema of the graph. For a _descending_ Morse complex,
/// the partitions correspond to maxima, while for an _ascending_ Morse complex,
/// the partitions correspond to minima.
///
/// Computing the Morse complex of a graph necessarily involves computing the
/// _persistence_ of the extrema in the graph. This persistence value is 
/// essentially a quantification of how topologically important that extrema
/// is in the graph, with more "important" extrema having higher persistence.
///
/// The partitions can then be combined with the persistence values to create a 
/// sequence of simplifications of the complex. This is known as a filtration
/// sequence. When computing the filtration sequence, the partitions are merged
/// according to their extrema's persistence, starting with the least persistent
/// partition. 
///
#[derive(Debug)]
pub struct MorseComplex {
    ordered_points: Vec<MorseNode>,
    cells: PointedUnionFind,
    pub filtration: Vec<MorseFiltrationStep>,
    kind: MorseKind
}

impl MorseComplex {
    fn from_graph<T>(kind: MorseKind, graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<MorseComplex, MorseError> {
        let ordered_points = MorseComplex::get_ordered_points(kind, &graph)?;
        let num_points = ordered_points.len();
        let cells = PointedUnionFind::new(num_points);
        let mut complex = MorseComplex{kind, ordered_points, cells, filtration: vec![]};
        complex.construct_complex(graph)?;
        Ok(complex)
    }

    fn get_ordered_points<T>(kind: MorseKind,
                             graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<Vec<MorseNode>, MorseError> {
        let nodes: Result<Vec<(NodeIndex, f64)>, MorseError> = graph.node_indices()
            .map(|node_idx| {
                match graph.node_weight(node_idx) {
                    None => Err(MorseError::MissingNode{node: node_idx}),
                    Some(weight) => {
                        if weight.value.is_nan() {
                            Err(MorseError::NanValue{node: node_idx})
                        } else{
                            Ok((node_idx, weight.value))
                        }
                    }
                }
            })
            .collect();
        let mut nodes = nodes?;

        nodes.sort_by(|(_, a), (_, b)| {
                // we know these aren't nan, but the compiler doesn't, so just handle nans
                // arbitrarily
                match kind {
                    MorseKind::Descending => match b.partial_cmp(&a) {
                        None => Ordering::Less,
                        Some(ord) => ord
                    },
                    MorseKind::Ascending => match a.partial_cmp(&b) {
                        None => Ordering::Less,
                        Some(ord) => ord
                    }
                }
            });
        Ok(nodes.iter().map(|(n, _)| MorseNode::new(*n)).collect())
    }

    fn compute_filtration(&self) -> Vec<MorseFiltrationStep> {
        let mut filtration = self.ordered_points.iter() 
            .filter_map(|point| {
                match point.data.as_ref() {
                    Some(data) => {
                        if let Some(parent) = data.merge_parent {
                            Some(MorseFiltrationStep{time: data.lifetime, destroyed_cell: point.node, owning_cell: parent})
                        } else {
                            None
                        }
                    }
                    None => None
                }
             })
             .collect::<Vec<_>>();
        // there _shouldn't_ be nans in here, looking forward to being confused in a month when
        // there are!
        filtration.sort_by(|a, b| match a.time.partial_cmp(&b.time) {
            None => Ordering::Less,
            Some(ord) => ord
        });
        filtration
    }

    /// Returns a HashMap mapping nodex to their Morse cell extrema
    pub fn get_complex(&self) -> HashMap<NodeIndex, NodeIndex> {
        self.ordered_points.iter() 
            .filter_map(|point| {
                match point.data.as_ref() {
                    Some(data) => Some((point.node, data.ancestor)),
                    None => None
                }
             })
             .collect()
    }

    /// Returns a mapping of NodeIndices to persistence values.
    ///
    /// Note that, by definition, global extrema have infinite persistence, and non-extrema have 0
    /// persistence
    pub fn get_persistence(&self) -> HashMap<NodeIndex, f64> {
        let mut result = HashMap::with_capacity(self.ordered_points.len());
        for morse_node in self.ordered_points.iter() {
            if let Some(data) = &morse_node.data {
                result.insert(morse_node.node, data.lifetime);
            }         
        }
        result
    }

    fn construct_complex<T>(&mut self, graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<&Self, MorseError>{
        // We iterate through the points in descending (or ascending, depends on self.kind) 
        // order, which means we are essentially building the morse complex at the same time
        // that we compute persistence.

        let inverse_lookup: HashMap<NodeIndex, usize> = self.ordered_points.iter().enumerate()
            .map(|x| (x.1.node, x.0))
            .collect();

        for i in 0..self.ordered_points.len() {
            // find all *already processed* points that we have an edge to
            let this_value = match graph.node_weight(self.ordered_points[i].node) {
                None => return Err(MorseError::MissingNode{node: self.ordered_points[i].node}),
                Some(weight) => weight.value
            };
            let higher_indices: Result<Vec<usize>, MorseError> = graph.neighbors(self.ordered_points[i].node)
                .filter(|n| { 
                    // I don't love silently dropping missing node weights, but the problem will
                    // throw an error farther down the line
                    let value = match graph.node_weight(*n) {
                        None => return false,
                        Some(weight) => weight.value
                    };
                    match self.kind {
                        MorseKind::Ascending => value <= this_value,
                        MorseKind::Descending => value >= this_value
                    }
                })
                .map(|n| match inverse_lookup.get(&n) {
                    None => Err(MorseError::MissingNode{node: n}),
                    Some(&n_idx) => Ok(n_idx)
                })
                .filter(|n_idx| match n_idx {
                    Err(_) => true,
                    Ok(n_idx) => *n_idx < i
                })
                .collect();
            let higher_indices = higher_indices?;

            // Nothing to do if we have no neighbors, but if we do then we
            // have to merge the correspond morse cells
            let lifetime = if higher_indices.is_empty () {
                f64::INFINITY  
            } else {
                0.
            };
            let ancestor = self.add_point_to_complex(i, &higher_indices, graph)?;

            // this is not a maximum so it has no lifetime
            self.ordered_points[i].data = Some(MorseData{lifetime, ancestor, merge_parent: None});
        }
        self.filtration = self.compute_filtration();
        Ok(self)
    }

    // FIXME: I don't like this signature. Not at all clear what this returned nodeindex means
    // FIXME: another type issue: usize gets used in two different ways (as cell and as index into
    // ordered_points). Would be good to clarify which was which
    fn add_point_to_complex<T>(&mut self, ordered_index: usize, ascending_neighbors: &[usize],
                      graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<NodeIndex, MorseError> {
        // If there are no neighbors, there's nothing to merge
        if ascending_neighbors.is_empty() {
            return Ok(self.ordered_points[ordered_index].node);
        }

        // one neighbor is easy, just union this point in to that neighbor's cell
        if ascending_neighbors.len() == 1 {
            let neighbor_index = ascending_neighbors[0];
            self.cells.union(neighbor_index, ordered_index);
            let neighbor = &self.ordered_points[neighbor_index];
            return match neighbor.data.as_ref() {
                None => Err(MorseError::MissingData{node: neighbor.node}),
                Some(data) => Ok(data.ancestor)
            }
        }

        // for multiple neighbors, first figure out if all neighbors are in the same cell
        let connected_cells: HashSet<_> = ascending_neighbors.iter()
            .map(|&idx| self.cells.find(idx))
            .collect();

        // If they are all in the same cell, it's the same as if there was just one neighbor
        if connected_cells.len() == 1 {
            let neighbor_index = ascending_neighbors[0];
            self.cells.union(neighbor_index, ordered_index);
            let neighbor = &self.ordered_points[neighbor_index];
            return match neighbor.data.as_ref() {
                None => Err(MorseError::MissingData{node: neighbor.node}),
                Some(data) => Ok(data.ancestor)
            }
        }

        // And if we're here then we're merging cells
        // first figure out what the global max is
        let max_cell = self.find_max_cell(ordered_index, &connected_cells, graph)?;
        let steepest_neighbor = self.find_steepest_neighbor(ordered_index, ascending_neighbors, graph)?;
        self.merge_cells(ordered_index, max_cell, &connected_cells, graph)?;
        let ancestor = &self.ordered_points[steepest_neighbor];

        match ancestor.data.as_ref() {
            None => Err(MorseError::MissingData{node: ancestor.node}),
            Some(data) => Ok(data.ancestor)
        }
    }

    fn find_max_cell<T>(&self, joining_index: usize, connected_cells: &HashSet<usize>, 
                        graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<usize, MorseError> {
        let mut current_max = None;
        let mut max_index = Err(MorseError::NoMaximum{node: self.ordered_points[joining_index].node});
        for &cell_index in connected_cells {
            let node = self.ordered_points[cell_index].node;
            let value = match graph.node_weight(node) {
                None => return Err(MorseError::MissingNode{node}),
                Some(weight) => weight.value
            };
            let should_update = match current_max {
                None => true,
                Some(max_val) => match self.kind {
                        MorseKind::Descending => value > max_val,
                        MorseKind::Ascending => value < max_val
                    }
                };
            if should_update {
                current_max = Some(value);
                max_index = Ok(cell_index);
            }
        }
        max_index
    }

    fn find_steepest_neighbor<T>(&self, joining_index: usize, neighbors: &[usize],
                                 graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<usize, MorseError> {
        // TODO: Really similar logic here and in max cell. Could probably unify them
        // NB this doesn't check signs; it assumes neighbors has been filtered appropriately
        let joining_node = &self.ordered_points[joining_index];
        let mut current_max = None;
        let mut max_index = Err(MorseError::MissingNeighbors{node: joining_node.node});
        for &neighbor_idx in neighbors {
            let node = &self.ordered_points[neighbor_idx];
            let value = match graph.node_weight(node.node) {
                None => return Err(MorseError::MissingNode{node: node.node}),
                Some(weight) => weight.value
            };
            let edge = match graph.find_edge(joining_node.node, node.node) {
                None => return Err(MorseError::MissingEdge{node: joining_node.node, other: node.node}),
                Some(edge) => edge
            };
            let grade = match graph.edge_weight(edge) {
                None => return Err(MorseError::MissingEdgeWeight{edge}),
                Some(val) => (value / val).abs()
            };

            let should_update = match current_max {
                None => true,
                Some(max_val) => grade > max_val
            };
            if should_update {
                current_max = Some(grade);
                max_index = Ok(neighbor_idx);
            }
        }
        max_index
    }

    fn merge_cells<T>(&mut self, joining_index: usize, owning_cell: usize, merged_cells: &HashSet<usize>,
                      graph: &UnGraph<LabeledPoint<T>, f64>) -> Result<(), MorseError> {
        let merge_parent = self.ordered_points[owning_cell].node;
        let joining_node = self.ordered_points[joining_index].node;
        let joining_value = match graph.node_weight(joining_node) {
            None => return Err(MorseError::MissingNode{node: joining_node}),
            Some(weight) => weight.value
        };
        self.cells.union(owning_cell, joining_index);
        for &cell in merged_cells {
            if cell != owning_cell {
                let cell_node = &self.ordered_points[cell];
                let cell_value = match graph.node_weight(cell_node.node) {
                    None => return Err(MorseError::MissingNode{node: cell_node.node}),
                    Some(weight) => weight.value
                };
                let ancestor = match self.ordered_points[cell].data.as_ref() {
                    None => return Err(MorseError::MissingData{node: cell_node.node}),
                    Some(data) => data.ancestor
                };

                // abs here so that the math works for ascending or descending
                let lifetime = (cell_value - joining_value).abs();
                self.ordered_points[cell].data = Some(MorseData{ancestor, lifetime, 
                    merge_parent: Some(merge_parent)});
                self.cells.union(owning_cell, cell);
            }
        }
        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single() {
        let mut graph = UnGraph::new_undirected();
        let points = [
            LabeledPoint{id: 0, value: -1., point: vec![0., 0.]},
            LabeledPoint{id: 1, value: 1., point: vec![1., 0.]},
        ];
        let mut node_lookup = Vec::with_capacity(points.len());
        for point in &points {
            let node = graph.add_node(point.to_owned());
            node_lookup.push(node);
        }
        graph.add_edge(node_lookup[0], node_lookup[1], 0.);
        let complex = MorseComplex::from_graph(MorseKind::Descending, &graph).unwrap();
        let lifetimes = complex.get_persistence();
        assert_eq!(lifetimes[&node_lookup[0]], 0.);
        assert_eq!(lifetimes[&node_lookup[1]], f64::INFINITY);
    }

    #[test]
    fn test_triangle() {
        let mut graph = UnGraph::new_undirected();
        let points = [
            LabeledPoint{id: 0, value: -1., point: vec![0., 0.]},
            LabeledPoint{id: 1, value: 0., point: vec![1., 1.]},
            LabeledPoint{id: 2, value: 1., point: vec![1., 0.]},
        ];
        let mut node_lookup = Vec::with_capacity(points.len());
        for point in &points {
            let node = graph.add_node(point.to_owned());
            node_lookup.push(node);
        }
        graph.add_edge(node_lookup[0], node_lookup[1], 0.);
        graph.add_edge(node_lookup[0], node_lookup[2], 0.);
        graph.add_edge(node_lookup[1], node_lookup[2], 0.);
        let complex = MorseComplex::from_graph(MorseKind::Descending, &graph).unwrap();
        let lifetimes = complex.get_persistence();
        assert_eq!(lifetimes[&node_lookup[0]], 0.);
        assert_eq!(lifetimes[&node_lookup[1]], 0.);
        assert_eq!(lifetimes[&node_lookup[2]], f64::INFINITY);
    }

    #[test]
    fn test_square() {
        let mut graph = UnGraph::new_undirected();
        let points = [
            LabeledPoint{id: 0, value: 1., point: vec![0., 0.]},
            LabeledPoint{id: 1, value: -1., point: vec![1., 0.]},
            LabeledPoint{id: 2, value: 0., point: vec![0., 1.]},
            LabeledPoint{id: 3, value: 2., point: vec![1., 1.]},
        ];
        let mut node_lookup = Vec::with_capacity(points.len());
        for point in &points {
            let node = graph.add_node(point.to_owned());
            node_lookup.push(node);
        }
        graph.add_edge(node_lookup[0], node_lookup[1], 0.);
        graph.add_edge(node_lookup[0], node_lookup[2], 0.);
        graph.add_edge(node_lookup[1], node_lookup[3], 0.);
        graph.add_edge(node_lookup[2], node_lookup[3], 0.);
        let complex = MorseComplex::from_graph(MorseKind::Descending, &graph).unwrap();
        let lifetimes = complex.get_persistence();
        assert_eq!(lifetimes[&node_lookup[0]], 1.);
        assert_eq!(lifetimes[&node_lookup[1]], 0.);
        assert_eq!(lifetimes[&node_lookup[2]], 0.);
        assert_eq!(lifetimes[&node_lookup[3]], f64::INFINITY);
    }

    #[test]
    fn test_all_equal_values() {
        let mut graph = UnGraph::new_undirected();
        let points = [
            LabeledPoint{id: 0, value: 0., point: vec![0., 0.]},
            LabeledPoint{id: 1, value: 0., point: vec![1., 0.]},
            LabeledPoint{id: 2, value: 0., point: vec![0., 1.]},
            LabeledPoint{id: 3, value: 0., point: vec![1., 1.]},
            LabeledPoint{id: 4, value: 1., point: vec![1., 1.]},
        ];
        let mut node_lookup = Vec::with_capacity(points.len());
        for point in &points {
            let node = graph.add_node(point.to_owned());
            node_lookup.push(node);
        }
        graph.add_edge(node_lookup[0], node_lookup[1], 1.);
        graph.add_edge(node_lookup[0], node_lookup[2], 1.);
        graph.add_edge(node_lookup[1], node_lookup[3], 1.);
        graph.add_edge(node_lookup[2], node_lookup[3], 1.);
        graph.add_edge(node_lookup[2], node_lookup[4], 1.);
        let complex = MorseComplex::from_graph(MorseKind::Descending, &graph).unwrap();
        let lifetimes = complex.get_persistence();
        println!("{:?}", lifetimes);
        assert_eq!(lifetimes[&node_lookup[0]], 0.);
        assert_eq!(lifetimes[&node_lookup[1]], 0.);
        assert_eq!(lifetimes[&node_lookup[2]], 0.);
        assert_eq!(lifetimes[&node_lookup[3]], 0.);
        assert_eq!(lifetimes[&node_lookup[4]], f64::INFINITY);
    }

    #[test]
    fn test_big_square_morse_smale() {
        let mut graph = UnGraph::new_undirected();
        let points = [
            LabeledPoint{id: 0, value: 6., point: vec![0., 0.]},
            LabeledPoint{id: 1, value: 2., point: vec![1., 0.]},
            LabeledPoint{id: 2, value: 3., point: vec![2., 0.]},
            LabeledPoint{id: 3, value: 5., point: vec![0., 1.]},
            LabeledPoint{id: 4, value: 4., point: vec![1., 1.]},
            LabeledPoint{id: 5, value: -5., point: vec![1., 2.]},
            LabeledPoint{id: 6, value: 0., point: vec![0., 2.]},
            LabeledPoint{id: 7, value: 1., point: vec![1., 2.]},
            LabeledPoint{id: 8, value: 10., point: vec![2., 2.]},
        ];
        let mut node_lookup = Vec::with_capacity(points.len());
        for point in &points {
            let node = graph.add_node(point.to_owned());
            node_lookup.push(node);
        }
        graph.add_edge(node_lookup[0], node_lookup[1], 1.);
        graph.add_edge(node_lookup[1], node_lookup[2], 1.);
        graph.add_edge(node_lookup[0], node_lookup[3], 1.);
        graph.add_edge(node_lookup[1], node_lookup[4], 1.);
        graph.add_edge(node_lookup[2], node_lookup[5], 1.);
        graph.add_edge(node_lookup[3], node_lookup[4], 1.);
        graph.add_edge(node_lookup[4], node_lookup[5], 1.);
        graph.add_edge(node_lookup[3], node_lookup[6], 1.);
        graph.add_edge(node_lookup[4], node_lookup[7], 1.);
        graph.add_edge(node_lookup[5], node_lookup[8], 1.);
        graph.add_edge(node_lookup[6], node_lookup[7], 1.);
        graph.add_edge(node_lookup[7], node_lookup[8], 1.);
        let complex = MorseSmaleComplex::from_graph(&graph).unwrap();
        let lifetimes = complex.descending_complex.get_persistence();
        assert_eq!(lifetimes[&node_lookup[0]], 5.);
        assert_eq!(lifetimes[&node_lookup[1]], 0.);
        assert_eq!(lifetimes[&node_lookup[2]], 1.);
        assert_eq!(lifetimes[&node_lookup[3]], 0.);
        assert_eq!(lifetimes[&node_lookup[4]], 0.);
        assert_eq!(lifetimes[&node_lookup[5]], 0.);
        assert_eq!(lifetimes[&node_lookup[6]], 0.);
        assert_eq!(lifetimes[&node_lookup[7]], 0.);
        assert_eq!(lifetimes[&node_lookup[8]], f64::INFINITY);

        let lifetimes = complex.ascending_complex.get_persistence();
        println!("{:?}", lifetimes);
        assert_eq!(lifetimes[&node_lookup[0]], 0.);
        assert_eq!(lifetimes[&node_lookup[1]], 1.);
        assert_eq!(lifetimes[&node_lookup[2]], 0.);
        assert_eq!(lifetimes[&node_lookup[3]], 0.);
        assert_eq!(lifetimes[&node_lookup[4]], 0.);
        assert_eq!(lifetimes[&node_lookup[5]], f64::INFINITY);
        assert_eq!(lifetimes[&node_lookup[6]], 4.);
        assert_eq!(lifetimes[&node_lookup[7]], 0.);
        assert_eq!(lifetimes[&node_lookup[8]], 0.);
    }

    #[test]
    fn test_filtration() {
        let mut graph = UnGraph::new_undirected();
        let points = [
            LabeledPoint{id: 0, value: 3., point: vec![0., 0.]},
            LabeledPoint{id: 1, value: -1., point: vec![1., 0.]},
            LabeledPoint{id: 2, value: 10., point: vec![0., 1.]},
            LabeledPoint{id: 3, value: 2., point: vec![1., 1.]},
            LabeledPoint{id: 4, value: 7., point: vec![1., 1.]},
        ];
        let mut node_lookup = Vec::with_capacity(points.len());
        for point in &points {
            let node = graph.add_node(point.to_owned());
            node_lookup.push(node);
        }
        graph.add_edge(node_lookup[0], node_lookup[1], 0.);
        graph.add_edge(node_lookup[0], node_lookup[3], 0.);
        graph.add_edge(node_lookup[1], node_lookup[2], 0.);
        graph.add_edge(node_lookup[1], node_lookup[4], 0.);
        graph.add_edge(node_lookup[3], node_lookup[4], 0.);
        let complex = MorseComplex::from_graph(MorseKind::Descending, &graph).unwrap();
        let lifetimes = complex.get_persistence();
        println!("{:?}", lifetimes);
        assert_eq!(lifetimes[&node_lookup[0]], 1.);
        assert_eq!(lifetimes[&node_lookup[1]], 0.);
        assert_eq!(lifetimes[&node_lookup[2]], f64::INFINITY);
        assert_eq!(lifetimes[&node_lookup[3]], 0.);
        assert_eq!(lifetimes[&node_lookup[4]], 8.);

        let filtration = complex.filtration;
        let expected = [(1., node_lookup[0], node_lookup[4]), (8., node_lookup[4], node_lookup[2])];
        for (actual, expected) in filtration.iter().zip(expected.iter()) {
            assert_eq!(actual.time, expected.0);
            assert_eq!(actual.destroyed_cell, expected.1);
            assert_eq!(actual.owning_cell, expected.2);
        }
    }
}
