//! Evidence-bearing structural graph independent of adapter representations.

use std::collections::HashMap;

use lattice_model::{Confidence, EvidenceOrigin, SourceSpan};
use lattice_resource::ResourceRef;
use petgraph::visit::EdgeRef;
use petgraph::{Direction, stable_graph::StableDiGraph};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub resource_ref: ResourceRef,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeKind {
    ParentOf,
    Requires,
    Calls,
    Reads,
    Writes,
    References,
    FiresRemote,
    InvokesRemote,
    HandlesRemote,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: EdgeKind,
    pub origin: EvidenceOrigin,
    pub confidence: Confidence,
    pub source_span: Option<SourceSpan>,
    pub revision: u64,
}

#[derive(Default)]
pub struct ProjectGraph {
    graph: StableDiGraph<GraphNode, GraphEdge>,
    nodes: HashMap<ResourceRef, petgraph::graph::NodeIndex>,
}

impl ProjectGraph {
    pub fn upsert_node(&mut self, node: GraphNode) {
        if let Some(index) = self.nodes.get(&node.resource_ref).copied() {
            self.graph[index] = node;
        } else {
            let reference = node.resource_ref.clone();
            let index = self.graph.add_node(node);
            self.nodes.insert(reference, index);
        }
    }

    pub fn add_edge(
        &mut self,
        source: &ResourceRef,
        target: &ResourceRef,
        edge: GraphEdge,
    ) -> bool {
        let (Some(source), Some(target)) = (self.nodes.get(source), self.nodes.get(target)) else {
            return false;
        };
        self.graph.add_edge(*source, *target, edge);
        true
    }

    #[must_use]
    pub fn dependencies(&self, source: &ResourceRef) -> Vec<(&GraphNode, &GraphEdge)> {
        let Some(index) = self.nodes.get(source).copied() else {
            return Vec::new();
        };
        self.graph
            .edges_directed(index, Direction::Outgoing)
            .map(|edge| (&self.graph[edge.target()], edge.weight()))
            .collect()
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_resource::{LatticeId, ResourceKind};

    #[test]
    fn graph_edges_always_carry_provenance() {
        let workspace = LatticeId::new();
        let left = ResourceRef::workspace(workspace, ResourceKind::Script, LatticeId::new());
        let right = ResourceRef::workspace(workspace, ResourceKind::Script, LatticeId::new());
        let mut graph = ProjectGraph::default();
        graph.upsert_node(GraphNode { resource_ref: left.clone(), name: "A".into() });
        graph.upsert_node(GraphNode { resource_ref: right.clone(), name: "B".into() });
        assert!(graph.add_edge(
            &left,
            &right,
            GraphEdge {
                kind: EdgeKind::Requires,
                origin: EvidenceOrigin::StaticAst,
                confidence: Confidence::Certain,
                source_span: None,
                revision: 1,
            },
        ));
        let dependencies = graph.dependencies(&left);
        assert_eq!(dependencies[0].0.name, "B");
        assert_eq!(dependencies[0].1.origin, EvidenceOrigin::StaticAst);
    }
}
