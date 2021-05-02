use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use crate::{
    Edge, EdgeError, EdgeKind, Entity, EntityError, ModelError, Node, NodeError, NodePosition,
    NodePositionError, SiStorable,
};
use si_data::PgTxn;

#[derive(Error, Debug)]
pub enum SchematicError {
    #[error("error in core model functions: {0}")]
    Model(#[from] ModelError),
    #[error("pg error: {0}")]
    TokioPg(#[from] tokio_postgres::Error),
    #[error("serde error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("entity error: {0}")]
    Entity(#[from] EntityError),
    #[error("edge error: {0}")]
    Edge(#[from] EdgeError),
    #[error("node error: {0}")]
    Node(#[from] NodeError),
    #[error("node position error: {0}")]
    NodePosition(#[from] NodePositionError),
    #[error("no change set provided when one was needed")]
    NoChangeSet,
    #[error("node is missing in calculated schematic edge set")]
    MissingNode,
}

pub type SchematicResult<T> = Result<T, SchematicError>;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum SchematicKind {
    System,
    Deployment,
    Component,
    Implementation,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeWithPositions {
    pub id: String,
    pub object_type: String,
    pub object_id: String,
    pub positions: HashMap<String, NodePosition>,
    pub si_storable: SiStorable,
}

impl NodeWithPositions {
    pub async fn from_node_position(txn: &PgTxn<'_>, node: Node) -> SchematicResult<Self> {
        let mut positions = HashMap::new();
        for node_position in NodePosition::get_by_node_id(&txn, &node.id)
            .await?
            .into_iter()
        {
            positions.insert(node_position.context_id.clone(), node_position);
        }

        Ok(Self {
            id: node.id,
            object_type: node.object_type,
            object_id: node.object_id,
            positions,
            si_storable: node.si_storable,
        })
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEdge {
    pub edge_id: String,
    pub node_id: String,
    pub socket_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Connections {
    pub predecessors: HashMap<String, Vec<ConnectionEdge>>,
    pub successors: HashMap<String, Vec<ConnectionEdge>>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum SocketKind {
    Input,
    Output,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum SocketType {
    Object,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Socket {
    pub id: String,
    pub socket_kind: SocketKind,
    pub socket_type: SocketType,
    pub object_type: Option<String>,
}

// Translating from schematic node sockets from the typescript model
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SchematicNodeSockets {
    pub inputs: Vec<Socket>,
    pub outputs: Vec<Socket>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SchematicNode {
    pub node: NodeWithPositions,
    pub sockets: SchematicNodeSockets,
    pub object: Entity,
    pub connections: Connections,
}

impl SchematicNode {
    pub async fn new(txn: &PgTxn<'_>, node: Node, object: Entity) -> SchematicResult<Self> {
        let node = NodeWithPositions::from_node_position(&txn, node).await?;

        let sockets = SchematicNodeSockets {
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        let connections = Connections {
            predecessors: HashMap::new(),
            successors: HashMap::new(),
        };

        Ok(Self {
            node,
            object,
            sockets,
            connections,
        })
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Schematic {
    // nodeId -> schematicNode
    pub nodes: HashMap<String, SchematicNode>,
    // edgeId -> Edge
    pub edges: HashMap<String, Edge>,
}

impl Schematic {
    pub async fn get_by_schematic_kind(
        txn: &PgTxn<'_>,
        schematic_kind: &SchematicKind,
        root_object_id: impl AsRef<str>,
        change_set_id: Option<String>,
        edit_session_id: Option<String>,
    ) -> SchematicResult<Schematic> {
        let root_object_id = root_object_id.as_ref();
        let mut schematic = match schematic_kind {
            SchematicKind::Deployment => {
                Schematic::get_deployment(&txn, root_object_id, change_set_id, edit_session_id)
                    .await?
            }

            SchematicKind::Component => {
                Schematic::get_component(&txn, root_object_id, change_set_id, edit_session_id)
                    .await?
            }

            _ => {
                Schematic::get(
                    &txn,
                    root_object_id,
                    change_set_id,
                    edit_session_id,
                    vec![
                        EdgeKind::Configures,
                        EdgeKind::Deployment,
                        EdgeKind::Implementation,
                    ],
                    // vec![EdgeKind::Configures, EdgeKind::Deployment, EdgeKind::Implementation],
                )
                .await?
            }
        };

        if schematic_kind == &SchematicKind::Deployment {
            let root_node = Node::get_for_object_id(&txn, &root_object_id, None).await?;
            schematic.prune_node(root_node.id);
        }

        return Ok(schematic);
    }

    pub async fn get_component(
        txn: &PgTxn<'_>,
        deployment_entity_id: impl AsRef<str>,
        change_set_id: Option<String>,
        edit_session_id: Option<String>,
    ) -> SchematicResult<Schematic> {
        let deployment_entity_id = deployment_entity_id.as_ref();
        let deployment_entity = Entity::for_head_or_change_set_or_edit_session(
            &txn,
            &deployment_entity_id,
            change_set_id.as_ref(),
            edit_session_id.as_ref(),
        )
        .await?;
        let deployment_node = Node::get(&txn, &deployment_entity.node_id).await?;

        let mut edges: HashMap<String, Edge> = HashMap::new();
        let mut nodes: HashMap<String, SchematicNode> = HashMap::new();
        let mut object_id_set: Vec<String> = vec![deployment_entity_id.into()];

        let sn = SchematicNode::new(&txn, deployment_node.clone(), deployment_entity).await?;
        nodes.insert(deployment_node.id.clone(), sn);

        let all_node_edges = Edge::direct_successor_edges_by_node_id(
            &txn,
            &EdgeKind::Component,
            &deployment_node.id,
        )
        .await?;

        for node_edge in all_node_edges.into_iter() {
            let successor_entity_id = &node_edge.head_vertex.object_id;
            object_id_set.push(successor_entity_id.clone());
            let successor_entity = match Entity::for_head_or_change_set_or_edit_session(
                &txn,
                successor_entity_id,
                change_set_id.as_ref(),
                edit_session_id.as_ref(),
            )
            .await
            {
                Ok(entity) => entity,
                Err(_e) => continue,
            };
            //edges.insert(node_edge.id.clone(), node_edge.clone());
            let successor_node = Node::get(&txn, &successor_entity.node_id).await?;
            if let None = nodes.get(&successor_node.id) {
                let successor_node_id = successor_node.id.clone();
                let sn = SchematicNode::new(&txn, successor_node.clone(), successor_entity).await?;
                nodes.insert(successor_node_id.clone(), sn);
            };
        }

        let full_edges = Edge::by_kind_and_overlapping_object_id_sets(
            &txn,
            &EdgeKind::Configures,
            object_id_set,
        )
        .await?;

        for successor_edge in full_edges.into_iter() {
            let successor_entity_id = &successor_edge.head_vertex.object_id;
            let successor_entity = match Entity::for_head_or_change_set_or_edit_session(
                &txn,
                successor_entity_id,
                change_set_id.as_ref(),
                edit_session_id.as_ref(),
            )
            .await
            {
                Ok(entity) => entity,
                Err(_e) => continue,
            };
            edges.insert(successor_edge.id.clone(), successor_edge.clone());
            let successor_node = Node::get(&txn, &successor_entity.node_id).await?;
            let schematic_node = nodes
                .get_mut(&successor_node.id)
                .ok_or(SchematicError::MissingNode)?;

            // Add a predecessors entry for the edge that we're on
            schematic_node
                .connections
                .predecessors
                .entry(successor_edge.kind.to_string())
                .and_modify(|p| {
                    let connection_edge = ConnectionEdge {
                        edge_id: successor_edge.id.clone(),
                        node_id: successor_edge.tail_vertex.node_id.clone(),
                        socket_id: successor_edge.tail_vertex.socket.clone(),
                    };
                    p.push(connection_edge);
                })
                .or_insert_with(|| {
                    let connection_edge = ConnectionEdge {
                        edge_id: successor_edge.id.clone(),
                        node_id: successor_edge.tail_vertex.node_id.clone(),
                        socket_id: successor_edge.tail_vertex.socket.clone(),
                    };
                    let p = vec![connection_edge];
                    p
                });
            if successor_edge.head_vertex.node_id != successor_node.id {
                nodes
                    .entry(successor_edge.head_vertex.node_id.clone())
                    .and_modify(|ns| {
                        ns.connections
                            .successors
                            .entry(successor_edge.kind.to_string())
                            .and_modify(|s| {
                                let connection_edge = ConnectionEdge {
                                    edge_id: successor_edge.id.clone(),
                                    node_id: successor_edge.head_vertex.node_id.clone(),
                                    socket_id: successor_edge.head_vertex.socket.clone(),
                                };
                                s.push(connection_edge);
                            })
                            .or_insert_with(|| {
                                let connection_edge = ConnectionEdge {
                                    edge_id: successor_edge.id.clone(),
                                    node_id: successor_edge.head_vertex.node_id.clone(),
                                    socket_id: successor_edge.head_vertex.socket.clone(),
                                };
                                let s = vec![connection_edge];
                                s
                            });
                    });
            }
        }

        let schematic = Schematic { nodes, edges };
        Ok(schematic)
    }

    pub async fn get_deployment(
        txn: &PgTxn<'_>,
        application_entity_id: impl AsRef<str>,
        change_set_id: Option<String>,
        edit_session_id: Option<String>,
    ) -> SchematicResult<Schematic> {
        let application_entity_id = application_entity_id.as_ref();
        let application_entity = Entity::for_head_or_change_set_or_edit_session(
            &txn,
            &application_entity_id,
            change_set_id.as_ref(),
            edit_session_id.as_ref(),
        )
        .await?;
        let application_node = Node::get(&txn, &application_entity.node_id).await?;

        let mut edges: HashMap<String, Edge> = HashMap::new();
        let mut nodes: HashMap<String, SchematicNode> = HashMap::new();
        let mut object_id_set: Vec<String> = vec![application_entity_id.into()];

        let sn = SchematicNode::new(&txn, application_node.clone(), application_entity).await?;
        nodes.insert(application_node.id.clone(), sn);

        let all_node_edges = Edge::direct_successor_edges_by_node_id(
            &txn,
            &EdgeKind::Component,
            &application_node.id,
        )
        .await?;

        for node_edge in all_node_edges.into_iter() {
            let successor_entity_id = &node_edge.head_vertex.object_id;
            object_id_set.push(successor_entity_id.clone());
            let successor_entity = match Entity::for_head_or_change_set_or_edit_session(
                &txn,
                successor_entity_id,
                change_set_id.as_ref(),
                edit_session_id.as_ref(),
            )
            .await
            {
                Ok(entity) => entity,
                Err(_e) => continue,
            };
            //edges.insert(node_edge.id.clone(), node_edge.clone());
            let successor_node = Node::get(&txn, &successor_entity.node_id).await?;
            if let None = nodes.get(&successor_node.id) {
                let successor_node_id = successor_node.id.clone();
                let sn = SchematicNode::new(&txn, successor_node.clone(), successor_entity).await?;
                nodes.insert(successor_node_id.clone(), sn);
            };
        }

        let full_edges = Edge::by_kind_and_overlapping_object_id_sets(
            &txn,
            &EdgeKind::Deployment,
            object_id_set,
        )
        .await?;

        for successor_edge in full_edges.into_iter() {
            let successor_entity_id = &successor_edge.head_vertex.object_id;
            let successor_entity = match Entity::for_head_or_change_set_or_edit_session(
                &txn,
                successor_entity_id,
                change_set_id.as_ref(),
                edit_session_id.as_ref(),
            )
            .await
            {
                Ok(entity) => entity,
                Err(_e) => continue,
            };
            edges.insert(successor_edge.id.clone(), successor_edge.clone());
            let successor_node = Node::get(&txn, &successor_entity.node_id).await?;
            let schematic_node = nodes
                .get_mut(&successor_node.id)
                .ok_or(SchematicError::MissingNode)?;

            // Add a predecessors entry for the edge that we're on
            schematic_node
                .connections
                .predecessors
                .entry(successor_edge.kind.to_string())
                .and_modify(|p| {
                    let connection_edge = ConnectionEdge {
                        edge_id: successor_edge.id.clone(),
                        node_id: successor_edge.tail_vertex.node_id.clone(),
                        socket_id: successor_edge.tail_vertex.socket.clone(),
                    };
                    p.push(connection_edge);
                })
                .or_insert_with(|| {
                    let connection_edge = ConnectionEdge {
                        edge_id: successor_edge.id.clone(),
                        node_id: successor_edge.tail_vertex.node_id.clone(),
                        socket_id: successor_edge.tail_vertex.socket.clone(),
                    };
                    let p = vec![connection_edge];
                    p
                });
            if successor_edge.head_vertex.node_id != successor_node.id {
                nodes
                    .entry(successor_edge.head_vertex.node_id.clone())
                    .and_modify(|ns| {
                        ns.connections
                            .successors
                            .entry(successor_edge.kind.to_string())
                            .and_modify(|s| {
                                let connection_edge = ConnectionEdge {
                                    edge_id: successor_edge.id.clone(),
                                    node_id: successor_edge.head_vertex.node_id.clone(),
                                    socket_id: successor_edge.head_vertex.socket.clone(),
                                };
                                s.push(connection_edge);
                            })
                            .or_insert_with(|| {
                                let connection_edge = ConnectionEdge {
                                    edge_id: successor_edge.id.clone(),
                                    node_id: successor_edge.head_vertex.node_id.clone(),
                                    socket_id: successor_edge.head_vertex.socket.clone(),
                                };
                                let s = vec![connection_edge];
                                s
                            });
                    });
            }
        }

        let schematic = Schematic { nodes, edges };
        Ok(schematic)
    }

    pub async fn get(
        txn: &PgTxn<'_>,
        root_object_id: impl AsRef<str>,
        change_set_id: Option<String>,
        edit_session_id: Option<String>,
        edge_kinds: Vec<EdgeKind>,
    ) -> SchematicResult<Schematic> {
        // Get the root object
        // Get its descendent edges
        // Populate the data
        // Profit!
        let root_object_id = root_object_id.as_ref();
        let root_entity = Entity::for_head_or_change_set_or_edit_session(
            &txn,
            &root_object_id,
            change_set_id.as_ref(),
            edit_session_id.as_ref(),
        )
        .await?;
        let root_node = Node::get(&txn, &root_entity.node_id).await?;

        let mut edges: HashMap<String, Edge> = HashMap::new();
        let mut nodes: HashMap<String, SchematicNode> = HashMap::new();

        let sn = SchematicNode::new(&txn, root_node.clone(), root_entity).await?;
        nodes.insert(root_node.id.clone(), sn);

        // An edge is included only if the object it points to has a head or a projection for this
        // changeset, or edit session - otherwise, it doesn't exist in the schematic!
        let successor_edges =
            Edge::all_successor_edges_by_node_id_for_edge_kinds(&txn, &edge_kinds, &root_node.id)
                .await?;

        for successor_edge in successor_edges.into_iter() {
            let successor_entity_id = &successor_edge.head_vertex.object_id;
            let successor_entity = match Entity::for_head_or_change_set_or_edit_session(
                &txn,
                successor_entity_id,
                change_set_id.as_ref(),
                edit_session_id.as_ref(),
            )
            .await
            {
                Ok(entity) => entity,
                Err(_e) => continue,
            };
            edges.insert(successor_edge.id.clone(), successor_edge.clone());
            let successor_node = Node::get(&txn, &successor_entity.node_id).await?;
            let schematic_node = if let Some(schematic_node) = nodes.get_mut(&successor_node.id) {
                schematic_node
            } else {
                let successor_node_id = successor_node.id.clone();
                let sn = SchematicNode::new(&txn, successor_node.clone(), successor_entity).await?;
                nodes.insert(successor_node_id.clone(), sn);
                // You just inserted it.. so it's cool.
                nodes.get_mut(&successor_node_id).unwrap()
            };

            // Add a predecessors entry for the edge that we're on
            schematic_node
                .connections
                .predecessors
                .entry(successor_edge.kind.to_string())
                .and_modify(|p| {
                    let connection_edge = ConnectionEdge {
                        edge_id: successor_edge.id.clone(),
                        node_id: successor_edge.tail_vertex.node_id.clone(),
                        socket_id: successor_edge.tail_vertex.socket.clone(),
                    };
                    p.push(connection_edge);
                })
                .or_insert_with(|| {
                    let connection_edge = ConnectionEdge {
                        edge_id: successor_edge.id.clone(),
                        node_id: successor_edge.tail_vertex.node_id.clone(),
                        socket_id: successor_edge.tail_vertex.socket.clone(),
                    };
                    let p = vec![connection_edge];
                    p
                });
            if successor_edge.head_vertex.node_id != successor_node.id {
                nodes
                    .entry(successor_edge.head_vertex.node_id.clone())
                    .and_modify(|ns| {
                        ns.connections
                            .successors
                            .entry(successor_edge.kind.to_string())
                            .and_modify(|s| {
                                let connection_edge = ConnectionEdge {
                                    edge_id: successor_edge.id.clone(),
                                    node_id: successor_edge.head_vertex.node_id.clone(),
                                    socket_id: successor_edge.head_vertex.socket.clone(),
                                };
                                s.push(connection_edge);
                            })
                            .or_insert_with(|| {
                                let connection_edge = ConnectionEdge {
                                    edge_id: successor_edge.id.clone(),
                                    node_id: successor_edge.head_vertex.node_id.clone(),
                                    socket_id: successor_edge.head_vertex.socket.clone(),
                                };
                                let s = vec![connection_edge];
                                s
                            });
                    });
            }
        }

        let schematic = Schematic { nodes, edges };
        Ok(schematic)
    }

    pub fn prune_node(&mut self, prune_node_id: impl AsRef<str>) {
        let prune_node_id = prune_node_id.as_ref();

        // Remove any node entries with the prune node id
        self.nodes.retain(|key, _| key != prune_node_id);

        // Remove any successor/predecessor connections that refer to the prune node id
        for (_, node) in self.nodes.iter_mut() {
            for (_, connection_edges) in node.connections.predecessors.iter_mut() {
                // Remove any connection edges that refer to the prune node id
                connection_edges.retain(|connection_edge| connection_edge.node_id != prune_node_id);
            }
            // Remove any remaining empty arrays
            node.connections
                .predecessors
                .retain(|_, values| !values.is_empty());

            for (_, connection_edges) in node.connections.successors.iter_mut() {
                // Remove any connection edges that refer to the prune node id
                connection_edges.retain(|connection_edge| connection_edge.node_id != prune_node_id);
            }
            // Remove any remaining empty arrays
            node.connections
                .successors
                .retain(|_, values| !values.is_empty());
        }

        // Remove any edges whose tail or head vertex refers to the prune node id
        self.edges.retain(|_, edge| {
            edge.head_vertex.node_id != prune_node_id && edge.tail_vertex.node_id != prune_node_id
        });
    }
}
