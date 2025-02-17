// SPDX-FileCopyrightText: © 2025 Claudio Cicconetti <c.cicconetti@iit.cnr.it>
// SPDX-License-Identifier: MIT

#[derive(Debug, Clone)]
enum NodeType {
    /// Satellite node.
    SAT,
    /// On ground station.
    OGS,
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                NodeType::SAT => "SAT",
                NodeType::OGS => "OGS",
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct NodeWeight {
    /// Node type.
    node_type: NodeType,
    /// Number of memory qubits.
    memory_qubits: u32,
    /// Fidelity decay rate of a qubit in memory.
    decay_rate: f64,
    /// Entanglement swapping success probability.
    swapping_success_prob: f64,
    /// Number of detectors.
    detectors: u32,
    /// Number of transmitters, i.e., entangled photon source generators.
    transmitters: u32,
    /// Capacity of transmitters, i.e., rate at which they generate
    /// EPR pairs.
    capacity: f64,
}

impl std::fmt::Display for NodeWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.node_type)
    }
}

impl Default for NodeWeight {
    fn default() -> Self {
        Self {
            node_type: NodeType::SAT,
            memory_qubits: 1,
            decay_rate: 0.0,
            swapping_success_prob: 1.0,
            detectors: 1,
            transmitters: 1,
            capacity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialOrd, PartialEq)]
pub struct EdgeWeight {
    /// Distance between two nodes, in m.
    distance: f64,
}

impl std::fmt::Display for EdgeWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.distance)
    }
}

impl petgraph::algo::FloatMeasure for EdgeWeight {
    fn zero() -> Self {
        Self {
            distance: f64::zero(),
        }
    }

    fn infinite() -> Self {
        Self {
            distance: f64::infinite(),
        }
    }
}

impl std::ops::Add for EdgeWeight {
    type Output = EdgeWeight;

    fn add(self, rhs: Self) -> Self::Output {
        EdgeWeight {
            distance: self.distance + rhs.distance,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaticFidelities {
    /// One hop, orbit-to-orbit.
    pub f_o: f64,
    /// One hop, orbit-to-ground.
    pub f_g: f64,
    /// Two hops, orbit-to-orbit.
    pub f_oo: f64,
    /// Two hops, orbit-to-ground.
    pub f_og: f64,
    /// Two hops, ground-to-ground.
    pub f_gg: f64,
}

impl Default for StaticFidelities {
    fn default() -> Self {
        Self {
            f_o: 1.0,
            f_g: 1.0,
            f_oo: 1.0,
            f_og: 1.0,
            f_gg: 1.0,
        }
    }
}

macro_rules! valid_node {
    ($node:expr, $graph:expr) => {
        anyhow::ensure!(
            $node.index() < $graph.node_count(),
            "there's no node {:?} in the graph",
            $node
        );
        anyhow::ensure!(
            $graph.node_weight($node).is_some(),
            "there's no node weight associated with {:?} in the graph",
            $node
        );
    };
}

/// Undirected graph representing the physical topology of the network.
///
/// An edge is present if two nodes can establish a quantum/classical link
/// with one another.
///
/// A simple fidelity model for the EPR pairs generated is used, with fixed
/// values depending only on whether the generation is one or two hops and
/// if it is STA-STA or STA-OGS.
#[derive(Debug, Default)]
pub struct PhysicalTopology {
    pub graph: petgraph::Graph<NodeWeight, EdgeWeight, petgraph::Undirected, u32>,
    fidelities: StaticFidelities,
    paths: std::collections::HashMap<
        petgraph::graph::NodeIndex,
        petgraph::algo::bellman_ford::Paths<petgraph::graph::NodeIndex, EdgeWeight>,
    >,
}

impl PhysicalTopology {
    /// Return the distance from node u to node v, in m.
    /// The paths are computed in a lazy manner.
    fn distance(
        &mut self,
        u: petgraph::graph::NodeIndex,
        v: petgraph::graph::NodeIndex,
    ) -> anyhow::Result<f64> {
        valid_node!(u, self.graph);
        valid_node!(v, self.graph);
        if let Some(paths) = self.paths.get(&u) {
            if let Some(_pred) = paths.predecessors[v.index()] {
                Ok(paths.distances[v.index()].distance)
            } else {
                anyhow::bail!("no connection between {:?} and {:?}", u, v);
            }
        } else {
            match petgraph::algo::bellman_ford(&self.graph, u.into()) {
                Ok(paths) => {
                    self.paths.insert(u, paths);
                    self.distance(u, v)
                }
                Err(_err) => anyhow::bail!(
                    "cannot compute distance from {:?} to {:?}: negative cycle",
                    u,
                    v
                ),
            }
        }
    }

    /// Return the initial fidelity of the EPR pairs generated by the given
    /// transmitter towards the two nodes specified. Return error if `tx` does not
    /// have a transmitter or there is no edge between `tx` and `u` or `v`.
    ///
    /// Parameters:
    /// - `tx`: the node that generates EPR pairs
    /// - `u`: one of the nodes that receives one photon of the EPR pairs
    /// - `v`: the other one
    fn fidelity(&mut self, tx: u32, u: u32, v: u32) -> anyhow::Result<f64> {
        let tx = petgraph::graph::NodeIndex::from(tx);
        let u = petgraph::graph::NodeIndex::from(u);
        let v = petgraph::graph::NodeIndex::from(v);
        valid_node!(tx, self.graph);
        valid_node!(u, self.graph);
        valid_node!(v, self.graph);
        anyhow::ensure!(
            self.graph.node_weight(tx).unwrap().transmitters > 0,
            "there are no transmitters on board of {}",
            tx.index()
        );
        anyhow::ensure!(
            u != v,
            "rx nodes are the same: {} = {}",
            u.index(),
            v.index()
        );
        anyhow::ensure!(
            matches!(self.graph.node_weight(tx).unwrap().node_type, NodeType::SAT),
            "node is an OGS and cannot be a transmitter: {}",
            tx.index()
        );

        if tx == u {
            anyhow::ensure!(
                self.graph.find_edge(tx, v).is_some(),
                "there is no edge between nodes {} and {}",
                tx.index(),
                v.index()
            );
            match self.graph.node_weight(v).unwrap().node_type {
                NodeType::SAT => Ok(self.fidelities.f_o),
                NodeType::OGS => Ok(self.fidelities.f_g),
            }
        } else if tx == v {
            anyhow::ensure!(
                self.graph.find_edge(tx, u).is_some(),
                "there is no edge between nodes {} and {}",
                tx.index(),
                u.index()
            );
            match self.graph.node_weight(u).unwrap().node_type {
                NodeType::SAT => Ok(self.fidelities.f_o),
                NodeType::OGS => Ok(self.fidelities.f_g),
            }
        } else {
            anyhow::ensure!(
                self.graph.find_edge(tx, u).is_some(),
                "there is no edge between nodes {} and {}",
                tx.index(),
                u.index()
            );
            anyhow::ensure!(
                self.graph.find_edge(tx, v).is_some(),
                "there is no edge between nodes {} and {}",
                tx.index(),
                v.index()
            );
            match self.graph.node_weight(u).unwrap().node_type {
                NodeType::SAT => match self.graph.node_weight(v).unwrap().node_type {
                    NodeType::SAT => Ok(self.fidelities.f_oo),
                    NodeType::OGS => Ok(self.fidelities.f_og),
                },
                NodeType::OGS => match self.graph.node_weight(v).unwrap().node_type {
                    NodeType::SAT => Ok(self.fidelities.f_og),
                    NodeType::OGS => Ok(self.fidelities.f_gg),
                },
            }
        }
    }

    fn to_dot(&self) -> String {
        format!("{}", petgraph::dot::Dot::new(&self.graph))
    }

    /// Create a topology of default nodes with given distances.
    #[cfg(test)]
    fn from_distances(edges: Vec<(u32, u32, f64)>, fidelities: StaticFidelities) -> Self {
        let mut graph = petgraph::Graph::new_undirected();

        graph.extend_with_edges(edges.iter().map(|(u, v, distance)| {
            (
                *u,
                *v,
                EdgeWeight {
                    distance: *distance,
                },
            )
        }));
        Self {
            graph,
            fidelities,
            paths: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeType, PhysicalTopology, StaticFidelities};

    fn test_graph() -> PhysicalTopology {
        //
        //                ┌───┐        ┌───┐
        //         100    │   │  100   │   │   100
        //       ┌────────┤ 1 ├────────┤ 2 ├────────┐
        //       │        │   │        │   │        │
        //       │        └─┬─┘        └─┬─┘        │
        //     ┌─┴─┐        │            │        ┌─┴─┐
        //     │   │        │            │        │   │
        //     │ 0 │        │150         │150     │ 5 │
        //     │   │        │            │        │   │
        //     └─┬─┘        │            │        └─┬─┘
        //       │        ┌─┴─┐        ┌─┴─┐        │
        //       │  100   │   │  100   │   │  100   │
        //       └────────┤ 3 ├────────┤ 4 │────────┘
        //                │   │        │   │
        //                └───┘        └───┘
        //

        PhysicalTopology::from_distances(
            vec![
                (0, 1, 100.0),
                (1, 2, 100.0),
                (2, 5, 100.0),
                (0, 3, 100.0),
                (3, 4, 100.0),
                (4, 5, 100.0),
                (1, 3, 150.0),
                (2, 4, 150.0),
            ],
            StaticFidelities::default(),
        )
    }

    #[test]
    fn test_physical_topology_distance() -> anyhow::Result<()> {
        let mut graph = test_graph();

        assert_float_eq::assert_f64_near!(graph.distance(0.into(), 1.into()).unwrap(), 100.0);
        assert_float_eq::assert_f64_near!(graph.distance(0.into(), 2.into()).unwrap(), 200.0);
        assert_float_eq::assert_f64_near!(graph.distance(0.into(), 5.into()).unwrap(), 300.0);
        assert_float_eq::assert_f64_near!(graph.distance(1.into(), 3.into()).unwrap(), 150.0);
        assert_float_eq::assert_f64_near!(graph.distance(3.into(), 1.into()).unwrap(), 150.0);

        assert!(graph.distance(0.into(), 99.into()).is_err());
        assert!(graph.distance(99.into(), 0.into()).is_err());
        assert!(graph.distance(99.into(), 99.into()).is_err());

        Ok(())
    }

    #[test]
    fn test_physical_topology_dot() {
        let graph: PhysicalTopology = test_graph();
        println!("{}", graph.to_dot());
    }

    #[test]
    fn test_physical_topology_fidelities() {
        let fidelities = StaticFidelities {
            f_o: 0.6,
            f_g: 0.7,
            f_oo: 0.8,
            f_og: 0.9,
            f_gg: 1.0,
        };

        let mut topo = PhysicalTopology::from_distances(
            vec![
                (0, 1, 1.0),
                (0, 2, 1.0),
                (0, 3, 1.0),
                (0, 4, 1.0),
                (4, 5, 1.0),
            ],
            fidelities.clone(),
        );

        topo.graph.node_weight_mut(0.into()).unwrap().node_type = NodeType::SAT;
        topo.graph.node_weight_mut(1.into()).unwrap().node_type = NodeType::OGS;
        topo.graph.node_weight_mut(2.into()).unwrap().node_type = NodeType::OGS;
        topo.graph.node_weight_mut(3.into()).unwrap().node_type = NodeType::SAT;
        topo.graph.node_weight_mut(4.into()).unwrap().node_type = NodeType::SAT;
        topo.graph.node_weight_mut(5.into()).unwrap().node_type = NodeType::SAT;

        assert_eq!(fidelities.f_o, topo.fidelity(0, 0, 3).unwrap());
        assert_eq!(fidelities.f_o, topo.fidelity(0, 3, 0).unwrap());
        assert_eq!(fidelities.f_g, topo.fidelity(0, 0, 1).unwrap());
        assert_eq!(fidelities.f_g, topo.fidelity(0, 1, 0).unwrap());
        assert_eq!(fidelities.f_oo, topo.fidelity(0, 3, 4).unwrap());
        assert_eq!(fidelities.f_og, topo.fidelity(0, 1, 3).unwrap());
        assert_eq!(fidelities.f_gg, topo.fidelity(0, 1, 2).unwrap());

        assert!(topo.fidelity(0, 0, 5).is_err());
        assert!(topo.fidelity(0, 5, 0).is_err());
        assert!(topo.fidelity(0, 1, 5).is_err());
        assert!(topo.fidelity(0, 1, 1).is_err());
        assert!(topo.fidelity(0, 0, 0).is_err());
        assert!(topo.fidelity(0, 99, 1).is_err());
        assert!(topo.fidelity(99, 1, 2).is_err());
    }
}
