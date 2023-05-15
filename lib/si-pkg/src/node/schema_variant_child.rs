use std::io::{BufRead, Write};

use object_tree::{
    read_key_value_line, write_key_value_line, GraphError, NameStr, NodeChild, NodeKind,
    NodeWithChildren, ReadBytes, WriteBytes,
};
use serde::{Deserialize, Serialize};

use crate::{
    CommandFuncSpec, FuncDescriptionSpec, LeafFunctionSpec, PropSpec, SiPropFuncSpec, SocketSpec,
    WorkflowSpec,
};

use super::PkgNode;

const VARIANT_CHILD_TYPE_COMMAND_FUNCS: &str = "command_funcs";
const VARIANT_CHILD_TYPE_DOMAIN: &str = "domain";
const VARIANT_CHILD_TYPE_FUNC_DESCRIPTIONS: &str = "func_descriptions";
const VARIANT_CHILD_TYPE_LEAF_FUNCTIONS: &str = "leaf_functions";
const VARIANT_CHILD_TYPE_RESOURCE_VALUE: &str = "resource_value";
const VARIANT_CHILD_TYPE_SI_PROP_FUNCS: &str = "si_prop_funcs";
const VARIANT_CHILD_TYPE_SOCKETS: &str = "sockets";
const VARIANT_CHILD_TYPE_WORKFLOWS: &str = "workflows";

const KEY_KIND_STR: &str = "kind";

#[remain::sorted]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SchemaVariantChild {
    CommandFuncs(Vec<CommandFuncSpec>),
    Domain(PropSpec),
    FuncDescriptions(Vec<FuncDescriptionSpec>),
    LeafFunctions(Vec<LeafFunctionSpec>),
    ResourceValue(PropSpec),
    SiPropFuncs(Vec<SiPropFuncSpec>),
    Sockets(Vec<SocketSpec>),
    Workflows(Vec<WorkflowSpec>),
}

#[remain::sorted]
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum SchemaVariantChildNode {
    CommandFuncs,
    Domain,
    FuncDescriptions,
    LeafFunctions,
    ResourceValue,
    SiPropFuncs,
    Sockets,
    Workflows,
}

impl SchemaVariantChildNode {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::CommandFuncs => VARIANT_CHILD_TYPE_COMMAND_FUNCS,
            Self::Domain => VARIANT_CHILD_TYPE_DOMAIN,
            Self::FuncDescriptions => VARIANT_CHILD_TYPE_FUNC_DESCRIPTIONS,
            Self::LeafFunctions => VARIANT_CHILD_TYPE_LEAF_FUNCTIONS,
            Self::ResourceValue => VARIANT_CHILD_TYPE_RESOURCE_VALUE,
            Self::SiPropFuncs => VARIANT_CHILD_TYPE_SI_PROP_FUNCS,
            Self::Sockets => VARIANT_CHILD_TYPE_SOCKETS,
            Self::Workflows => VARIANT_CHILD_TYPE_WORKFLOWS,
        }
    }
}

impl NameStr for SchemaVariantChildNode {
    fn name(&self) -> &str {
        match self {
            Self::CommandFuncs => VARIANT_CHILD_TYPE_COMMAND_FUNCS,
            Self::Domain => VARIANT_CHILD_TYPE_DOMAIN,
            Self::FuncDescriptions => VARIANT_CHILD_TYPE_FUNC_DESCRIPTIONS,
            Self::LeafFunctions => VARIANT_CHILD_TYPE_LEAF_FUNCTIONS,
            Self::ResourceValue => VARIANT_CHILD_TYPE_RESOURCE_VALUE,
            Self::SiPropFuncs => VARIANT_CHILD_TYPE_SI_PROP_FUNCS,
            Self::Sockets => VARIANT_CHILD_TYPE_SOCKETS,
            Self::Workflows => VARIANT_CHILD_TYPE_WORKFLOWS,
        }
    }
}

impl WriteBytes for SchemaVariantChildNode {
    fn write_bytes<W: Write>(&self, writer: &mut W) -> Result<(), GraphError> {
        write_key_value_line(writer, KEY_KIND_STR, self.kind_str())?;
        Ok(())
    }
}

impl ReadBytes for SchemaVariantChildNode {
    fn read_bytes<R: BufRead>(reader: &mut R) -> Result<Self, GraphError>
    where
        Self: std::marker::Sized,
    {
        let kind_str = read_key_value_line(reader, KEY_KIND_STR)?;

        let node = match kind_str.as_str() {
            VARIANT_CHILD_TYPE_COMMAND_FUNCS => Self::CommandFuncs,
            VARIANT_CHILD_TYPE_DOMAIN => Self::Domain,
            VARIANT_CHILD_TYPE_FUNC_DESCRIPTIONS => Self::FuncDescriptions,
            VARIANT_CHILD_TYPE_LEAF_FUNCTIONS => Self::LeafFunctions,
            VARIANT_CHILD_TYPE_RESOURCE_VALUE => Self::ResourceValue,
            VARIANT_CHILD_TYPE_SI_PROP_FUNCS => Self::SiPropFuncs,
            VARIANT_CHILD_TYPE_SOCKETS => Self::Sockets,
            VARIANT_CHILD_TYPE_WORKFLOWS => Self::Workflows,
            invalid_kind => {
                return Err(GraphError::parse_custom(format!(
                    "invalid schema variant child kind: {invalid_kind}"
                )))
            }
        };

        Ok(node)
    }
}

impl NodeChild for SchemaVariantChild {
    type NodeType = PkgNode;

    fn as_node_with_children(&self) -> NodeWithChildren<Self::NodeType> {
        match self {
            Self::CommandFuncs(command_funcs) => NodeWithChildren::new(
                NodeKind::Tree,
                Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::CommandFuncs),
                command_funcs
                    .iter()
                    .map(|command_func| {
                        Box::new(command_func.clone())
                            as Box<dyn NodeChild<NodeType = Self::NodeType>>
                    })
                    .collect(),
            ),
            Self::Domain(domain) => {
                let domain =
                    Box::new(domain.clone()) as Box<dyn NodeChild<NodeType = Self::NodeType>>;

                NodeWithChildren::new(
                    NodeKind::Tree,
                    Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::Domain),
                    vec![domain],
                )
            }
            Self::ResourceValue(resource_value) => {
                let resource_value = Box::new(resource_value.clone())
                    as Box<dyn NodeChild<NodeType = Self::NodeType>>;

                NodeWithChildren::new(
                    NodeKind::Tree,
                    Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::ResourceValue),
                    vec![resource_value],
                )
            }
            Self::FuncDescriptions(descriptions) => NodeWithChildren::new(
                NodeKind::Tree,
                Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::FuncDescriptions),
                descriptions
                    .iter()
                    .map(|description| {
                        Box::new(description.clone())
                            as Box<dyn NodeChild<NodeType = Self::NodeType>>
                    })
                    .collect(),
            ),
            Self::LeafFunctions(entries) => NodeWithChildren::new(
                NodeKind::Tree,
                Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::LeafFunctions),
                entries
                    .iter()
                    .map(|entry| {
                        Box::new(entry.clone()) as Box<dyn NodeChild<NodeType = Self::NodeType>>
                    })
                    .collect(),
            ),
            Self::Sockets(sockets) => NodeWithChildren::new(
                NodeKind::Tree,
                Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::Sockets),
                sockets
                    .iter()
                    .map(|socket| {
                        Box::new(socket.clone()) as Box<dyn NodeChild<NodeType = Self::NodeType>>
                    })
                    .collect(),
            ),
            Self::SiPropFuncs(si_prop_funcs) => NodeWithChildren::new(
                NodeKind::Tree,
                Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::SiPropFuncs),
                si_prop_funcs
                    .iter()
                    .map(|si_prop_func| {
                        Box::new(si_prop_func.clone())
                            as Box<dyn NodeChild<NodeType = Self::NodeType>>
                    })
                    .collect(),
            ),
            Self::Workflows(workflows) => NodeWithChildren::new(
                NodeKind::Tree,
                Self::NodeType::SchemaVariantChild(SchemaVariantChildNode::Workflows),
                workflows
                    .iter()
                    .map(|workflow| {
                        Box::new(workflow.clone()) as Box<dyn NodeChild<NodeType = Self::NodeType>>
                    })
                    .collect(),
            ),
        }
    }
}