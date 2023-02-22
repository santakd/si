//! An implementation of a tree structure that can be transformed into a Merkle n-ary acyclic
//! directed graph (i.e. "DAG").

use std::{
    collections::HashMap,
    fmt,
    io::{self, BufRead, Cursor, Write},
    str::FromStr,
};

use petgraph::prelude::*;
use serde::Serialize;
use strum::{AsRefStr, EnumString};
use thiserror::Error;

use crate::Hash;

const KEY_VERSION_STR: &str = "version";
const KEY_NODE_KIND_STR: &str = "node_kind";
const KEY_OBJECT_KIND_STR: &str = "object_kind";

const VAL_VERSION_STR: &str = "1";

/// The canonical serialized form of a new line.
pub const NL: &str = "\n";

/// An error that can be returned when working with tree and graph types.
#[derive(Debug, Error)]
pub enum GraphError {
    /// When parsing a serialized node representation and a valid version was found
    #[error("invalid node version when parsing from bytes: {0}")]
    InvalidNodeVersion(String),
    /// When an error is returned while reading serialized node representation
    #[error("error reading node representation from bytes")]
    IoRead(#[source] io::Error),
    /// When an error is returned while writing a serialized node representation
    #[error("error writing node representation as bytes")]
    IoWrite(#[source] io::Error),
    /// When a root node was not found after traversing a tree
    #[error("root node not set after traversing tree")]
    MissingRootNode,
    /// When multiple root nodes were found while traversing a tree
    #[error("root node already set, cannot have multiple roots in tree")]
    MultipleRootNode,
    /// When parsing a serialized node from bytes returns an error
    #[error("error parsing node from bytes: {0}")]
    Parse(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// When parsing a serialized node from bytes and an invalid state is found
    #[error("error parsing node from bytes: {0}")]
    ParseCustom(String),
    /// When a blank line was expected while parsing a serialized node
    #[error("parsing line was expected to be blank, but got '{0}'")]
    ParseLineBlank(String),
    /// When a line was expected to contain a given key while parsing a serialized node
    #[error("parsing key/value line error, expected key '{0}', but got '{1}'")]
    ParseLineExpectedKey(String, String),
    /// When a line failed to parse as a key/value line while parsing a serialize node
    #[error("could not parse line as 'key=value': '{0}'")]
    ParseLineKeyValueFormat(String),
    /// When a child node is missing a hash value while computing a hashing tree
    #[error("unhashed child node for '{0}' with name: {1}")]
    UnhashedChild(String, String),
    /// When a node is missing a hash value while computing a hashing tree
    #[error("unhashed node with name: {0}")]
    UnhashedNode(String),
    /// When a hash value failed to verify an expected value
    #[error("failed to verify hash; expected={0}, computed={1}")]
    Verify(Hash, Hash),
}

impl GraphError {
    /// Returns a parsing error which wraps the given inner error.
    pub fn parse<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Parse(Box::new(err))
    }

    /// Return a custom parsing error which contains the given message.
    pub fn parse_custom(msg: impl Into<String>) -> Self {
        Self::ParseCustom(msg.into())
    }
}

/// Trait for types that can serialize to a representation of bytes.
pub trait WriteBytes {
    /// Writes a serialized version of `self` to the writer as bytes.
    fn write_bytes<W: Write>(&self, writer: &mut W) -> Result<(), GraphError>;

    /// Builds and returns a `Vec` of bytes which is a serialized representation of `self`.
    fn to_bytes(&self) -> Result<Vec<u8>, GraphError> {
        let mut writer = Cursor::new(Vec::new());
        self.write_bytes(&mut writer)?;
        Ok(writer.into_inner())
    }
}

/// Trait for types which can compute and verify their own [`struct@Hash`] value.
pub trait VerifyHash: WriteBytes {
    /// Returns a pre-computed [`struct@Hash`] value for `self`.
    fn hash(&self) -> &Hash;

    /// Recomputes a [`struct@Hash`] value for `self` and confirms it matches the pre-computed Hash
    /// value.
    fn verify_hash(&self) -> Result<(), GraphError> {
        let input = self.to_bytes()?;
        let computed = Hash::new(&input);

        if self.hash() == &computed {
            Ok(())
        } else {
            Err(GraphError::Verify(*self.hash(), computed))
        }
    }
}

/// Trait for types that can deserialize its representation from bytes.
pub trait ReadBytes {
    /// Reads a serialized version of `self` from a reader over bytes.
    fn read_bytes<R: BufRead>(reader: &mut R) -> Result<Self, GraphError>
    where
        Self: std::marker::Sized;

    /// Builds and returns a new instance which was deserialized from a `Vec` of bytes.
    fn from_bytes(buf: Vec<u8>) -> Result<Self, GraphError>
    where
        Self: std::marker::Sized,
    {
        let mut reader = Cursor::new(buf);
        Self::read_bytes(&mut reader)
    }
}

/// Trait for types that return a String representation of its name.
pub trait NameStr {
    /// Returns a name as a `&str`.
    fn name(&self) -> &str;
}

/// Trait for types that return a String representation of its kind or type.
// TODO(fnichol): I *think* this goes away--each inner `T` needs to know how to deserialize itself
// from bytes so shoult typically have a `kind`/`type` key/value line. The wrapping reader for the
// inner `T` can't give `T` the value of `object_kind()`--it can only call `T::read_bytes()` so
// what use is this then in the end? I think nothing, but since it was a hard trait to thread
// though, let's wait one more seconds before pulling it.
pub trait ObjectKindStr {
    /// Returns an object kind as a `&str`.
    fn object_kind(&self) -> &str;
}

/// Whether a `Node` (or a node-related type) is a leaf or a tree.
///
/// A *leaf* is a node which contains no children and a *tree* is a node which contains children.
#[derive(AsRefStr, Debug, Clone, Copy, EnumString, Eq, Hash, PartialEq, Serialize)]
#[strum(serialize_all = "camelCase")]
pub enum NodeKind {
    /// A leaf node has no children.
    Leaf,
    /// A tree node has children.
    Tree,
}

/// A node entry is a representation of a child node in a parent node's serialized representation.
#[derive(Clone, Debug)]
pub(crate) struct NodeEntry {
    kind: NodeKind,
    hash: Hash,
    name: String,
}

impl NodeEntry {
    pub(crate) fn new(kind: NodeKind, hash: Hash, name: impl Into<String>) -> Self {
        Self {
            kind,
            hash,
            name: name.into(),
        }
    }

    #[must_use]
    pub(crate) fn hash(&self) -> Hash {
        self.hash
    }
}

impl WriteBytes for NodeEntry {
    fn write_bytes<W: Write>(&self, writer: &mut W) -> Result<(), GraphError> {
        write!(
            writer,
            "{} {} {}{NL}",
            self.kind.as_ref(),
            self.hash,
            self.name
        )
        .map_err(GraphError::IoWrite)
    }
}

/// An un-hashed node in a tree.
#[derive(Clone, Debug)]
struct Node<T> {
    kind: NodeKind,
    inner: T,
}

/// An un-hashed tree node which includes its children.
pub struct NodeWithChildren<T, N>
where
    N: Into<NodeWithChildren<T, N>>,
{
    kind: NodeKind,
    inner: T,
    children: Vec<N>,
}

impl<T, N> NodeWithChildren<T, N>
where
    N: Into<NodeWithChildren<T, N>>,
{
    /// Creates a new instance given a kind, an inner type `T` and its children.
    pub fn new(kind: NodeKind, inner: T, children: Vec<N>) -> Self {
        Self {
            kind,
            inner,
            children,
        }
    }
}

impl<T, N> From<NodeWithChildren<T, N>> for (Node<T>, Vec<NodeWithChildren<T, N>>)
where
    N: Into<NodeWithChildren<T, N>>,
{
    fn from(value: NodeWithChildren<T, N>) -> Self {
        let node = Node {
            kind: value.kind,
            inner: value.inner,
        };
        let children = value.children.into_iter().map(Into::into).collect();

        (node, children)
    }
}

/// A reference to an un-hashed node which includes a slice of [`NodeEntry`] items representing its
/// children, if any.
struct NodeWithEntriesRef<'a, T> {
    kind: NodeKind,
    inner: &'a T,
    entries: &'a [NodeEntry],
}

impl<'a, T> NodeWithEntriesRef<'a, T> {
    /// Creates a new instance given a kind, an innter type `T` and a slice of [`NodeEntry`] items
    /// representing its children, if any.
    fn new(kind: NodeKind, inner: &'a T, entries: &'a [NodeEntry]) -> Self {
        Self {
            kind,
            inner,
            entries,
        }
    }
}

impl<'a, T> WriteBytes for NodeWithEntriesRef<'a, T>
where
    T: WriteBytes + ObjectKindStr,
{
    fn write_bytes<W: Write>(&self, writer: &mut W) -> Result<(), GraphError> {
        write_header_bytes(writer, self.kind, self.inner.object_kind())?;

        write_separator_bytes(writer)?;

        self.inner.write_bytes(writer)?;

        if !self.entries.is_empty() {
            write_separator_bytes(writer)?;

            // all entries must be deterministically ordered, and that is by entry name sorted
            // lexically
            let mut sorted_entries: Vec<_> = self.entries.iter().collect();
            sorted_entries.sort_by_key(|k| &k.name);

            for entry in sorted_entries {
                entry.write_bytes(writer)?;
            }
        }

        Ok(())
    }
}

/// An un-hashed node which includes a `Vec` of [`NodeEntry`] items representing its children, if
/// any.
pub(crate) struct NodeWithEntries<T> {
    kind: NodeKind,
    inner: T,
    entries: Vec<NodeEntry>,
}

impl<T> ReadBytes for NodeWithEntries<T>
where
    T: ReadBytes,
{
    fn read_bytes<R: BufRead>(reader: &mut R) -> Result<Self, GraphError>
    where
        Self: std::marker::Sized,
    {
        let version_str = read_key_value_line(reader, KEY_VERSION_STR)?;
        if version_str != VAL_VERSION_STR {
            return Err(GraphError::InvalidNodeVersion(version_str));
        }

        let kind_str = read_key_value_line(reader, KEY_NODE_KIND_STR)?;
        let kind = NodeKind::from_str(&kind_str).map_err(GraphError::parse)?;

        let object_kind_str = read_key_value_line(reader, KEY_OBJECT_KIND_STR)?;
        // TODO(fnichol): right now we're only round-tripping PropNodes, but soon others--this is a
        // pedantic check of this field in the meantime but will serve as the de-serializing hint
        if object_kind_str != "prop" {
            return Err(GraphError::parse_custom("expected object kind to be prop"));
        }

        read_empty_line(reader)?;

        let node = T::read_bytes(reader)?;

        let entries = match kind {
            NodeKind::Leaf => vec![],
            NodeKind::Tree => {
                read_empty_line(reader)?;

                read_node_entry_lines(reader)?
            }
        };

        Ok(Self {
            kind,
            inner: node,
            entries,
        })
    }
}

/// A tree structure that is used to compute a fully hashed Merkle DAG.
#[derive(Clone, Debug)]
struct HashingTree<T> {
    graph: Graph<Node<T>, ()>,
    root_idx: NodeIndex,
    hashes: HashMap<NodeIndex, Hash>,
}

impl<T> HashingTree<T> {
    /// Builds newa [`HashingTree`] from a root [`NodeWithChildren`] that can be hashed and
    /// computed.
    ///
    /// # Errors
    ///
    /// Return `Err` if multiple root nodes are found (which is invalid for a tree) or if no root
    /// nodes are found once the tree is fully processed (which is also invalid for a tree).
    fn create_from_root<N>(node: NodeWithChildren<T, N>) -> Result<HashingTree<T>, GraphError>
    where
        N: Into<NodeWithChildren<T, N>>,
    {
        let mut graph = Graph::new();
        let mut root_idx: Option<NodeIndex> = None;
        let hashes = HashMap::new();

        let mut stack: Vec<(_, Option<NodeIndex>)> = vec![(node, None)];

        while let Some((node_with_children, parent_idx)) = stack.pop() {
            let (node, children) = node_with_children.into();

            let node_idx = graph.add_node(node);

            match parent_idx {
                Some(parent_idx) => {
                    graph.add_edge(parent_idx, node_idx, ());
                }
                None => match root_idx {
                    None => {
                        root_idx = Some(node_idx);
                    }
                    Some(_) => return Err(GraphError::MultipleRootNode),
                },
            };

            for child_node_with_children in children.into_iter().rev() {
                stack.push((child_node_with_children, Some(node_idx)));
            }
        }

        match root_idx {
            Some(root_idx) => Ok(HashingTree {
                graph,
                root_idx,
                hashes,
            }),
            None => Err(GraphError::MissingRootNode),
        }
    }

    /// Builds a new [`ObjectTree`] by computing hashes for all nodes.
    ///
    /// # Errors
    ///
    /// Return `Err` if:
    ///
    /// - An un-hashed child node is found during depth-first post-order tree traversal (i.e. this
    /// implies all children have not yet been computed which is invalid)
    /// - An I/O error occurs when serializing node representations to bytes
    fn hash_tree(mut self) -> Result<ObjectTree<T>, GraphError>
    where
        T: Clone + NameStr + WriteBytes + ObjectKindStr,
    {
        self.compute_hashes()?;
        self.create_hashed_tree()
    }

    fn compute_hashes(&mut self) -> Result<(), GraphError>
    where
        T: NameStr + WriteBytes + ObjectKindStr,
    {
        let mut dfspo = DfsPostOrder::new(&self.graph, self.root_idx);

        while let Some(node_idx) = dfspo.next(&self.graph) {
            let node = &self.graph[node_idx];

            // Create an entry for each direct child
            let mut entries = Vec::new();
            for child_idx in self.graph.neighbors_directed(node_idx, Outgoing) {
                let child_node = &self.graph[child_idx];
                let child_hash = self.hashes.get(&child_idx).ok_or_else(|| {
                    GraphError::UnhashedChild(
                        node.inner.name().to_string(),
                        child_node.inner.name().to_string(),
                    )
                })?;

                entries.push(NodeEntry {
                    kind: child_node.kind,
                    hash: *child_hash,
                    name: child_node.inner.name().to_string(),
                });
            }

            // Serialize node to bytes and compute hash
            let mut writer = Cursor::new(Vec::new());
            NodeWithEntriesRef::new(node.kind, &node.inner, &entries).write_bytes(&mut writer)?;
            let computed_hash = Hash::new(&writer.into_inner());

            self.hashes.insert(node_idx, computed_hash);
        }

        Ok(())
    }

    fn create_hashed_tree(self) -> Result<ObjectTree<T>, GraphError>
    where
        T: Clone + NameStr,
    {
        #[derive(Debug)]
        struct StackEntry<T> {
            hashed_node: HashedNode<T>,
            other_idx: NodeIndex,
            parent_idx: Option<NodeIndex>,
        }

        let other_root_node = self.graph[self.root_idx].clone();
        let other_root_node_hash = self
            .hashes
            .get(&self.root_idx)
            .ok_or_else(|| GraphError::UnhashedNode(other_root_node.inner.name().to_string()))?;

        let mut graph = Graph::new();
        let mut root_idx: Option<NodeIndex> = None;

        let mut stack = vec![StackEntry {
            hashed_node: HashedNode::new(other_root_node, *other_root_node_hash),
            other_idx: self.root_idx,
            parent_idx: None,
        }];

        while let Some(entry) = stack.pop() {
            let node_idx = graph.add_node(entry.hashed_node);

            match entry.parent_idx {
                Some(parent_idx) => {
                    graph.add_edge(parent_idx, node_idx, ());
                }
                None => match root_idx {
                    None => {
                        root_idx = Some(node_idx);
                    }
                    Some(_) => return Err(GraphError::MultipleRootNode),
                },
            };

            for other_child_idx in self.graph.neighbors_directed(entry.other_idx, Outgoing) {
                let other_node = self.graph[other_child_idx].clone();
                let other_node_hash = self
                    .hashes
                    .get(&other_child_idx)
                    .ok_or_else(|| GraphError::UnhashedNode(other_node.inner.name().to_string()))?;

                stack.push(StackEntry {
                    hashed_node: HashedNode::new(other_node, *other_node_hash),
                    other_idx: other_child_idx,
                    parent_idx: Some(node_idx),
                });
            }
        }

        match root_idx {
            Some(root_idx) => Ok(ObjectTree { graph, root_idx }),
            None => Err(GraphError::MissingRootNode),
        }
    }
}

/// A tree of hashed nodes of type `T`.
///
/// The tree can be considered a Merkle DAG (directed acyclic graph) or a Merkle n-ary tree (that
/// is not a binary or "balanced" tree). A node is hashed over its serialized bytes representation
/// which includes the hashes of all of its children. In this way it is possible to determine if 2
/// nodes are equivalent in that they both represent identical sub-trees and can be mathematically
/// verified.
#[derive(Clone, Debug)]
pub struct ObjectTree<T> {
    graph: Graph<HashedNode<T>, ()>,
    root_idx: NodeIndex,
}

impl<T> ObjectTree<T> {
    /// Creates an `ObjectTree` from an un-hashed root node of type `T` with its children.
    pub fn create_from_root<N>(node: NodeWithChildren<T, N>) -> Result<Self, GraphError>
    where
        T: Clone + NameStr + WriteBytes + ObjectKindStr,
        N: Into<NodeWithChildren<T, N>>,
    {
        HashingTree::create_from_root(node)?.hash_tree()
    }

    /// Returns the tree as a [`Graph`] of [`HashedNode`] items and a pointer to the root node.
    pub fn as_petgraph(&self) -> (&Graph<HashedNode<T>, ()>, NodeIndex) {
        (&self.graph, self.root_idx)
    }

    /// Builds a new `ObjectTree` from an exisiting [`Graph`] of [`HashedNode`] items and a root
    /// index pointer.
    #[must_use]
    pub(crate) fn new(graph: Graph<HashedNode<T>, ()>, root_idx: NodeIndex) -> Self {
        Self { graph, root_idx }
    }
}

/// A hashed node of type `T`.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct HashedNode<T> {
    kind: NodeKind,
    hash: Hash,
    inner: T,
}

impl<T> fmt::Debug for HashedNode<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashedNode")
            .field("kind", &self.kind)
            // This is pragmatic--the full hashes can lead to very long output lines and visual Dot
            // graph images
            .field("hash", &self.hash.short_string())
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T> HashedNode<T> {
    fn new(node: Node<T>, hash: Hash) -> Self {
        Self {
            kind: node.kind,
            hash,
            inner: node.inner,
        }
    }

    /// Returns the [`NodeKind`] of this node.
    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    /// Returns the pre-computed [`struct@Hash`] of this node.
    pub fn hash(&self) -> Hash {
        self.hash
    }

    /// Returns the inner representation `T` of this node.
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T> NameStr for HashedNode<T>
where
    T: NameStr,
{
    fn name(&self) -> &str {
        self.inner.name()
    }
}

/// A hashed node which includes a `Vec` of [`NodeEntry`] items representing its children, if any.
pub(crate) struct HashedNodeWithEntries<T> {
    kind: NodeKind,
    hash: Hash,
    inner: T,
    entries: Vec<NodeEntry>,
}

impl<T> HashedNodeWithEntries<T> {
    pub(crate) fn new(hashed_node: HashedNode<T>, entries: Vec<NodeEntry>) -> Self {
        Self {
            kind: hashed_node.kind,
            hash: hashed_node.hash,
            inner: hashed_node.inner,
            entries,
        }
    }

    pub(crate) fn from_node_with_entries_and_hash(
        node_with_entries: NodeWithEntries<T>,
        hash: Hash,
    ) -> Self {
        Self {
            kind: node_with_entries.kind,
            hash,
            inner: node_with_entries.inner,
            entries: node_with_entries.entries,
        }
    }

    pub(crate) fn hash(&self) -> Hash {
        self.hash
    }

    fn as_node_with_entries_ref(&self) -> NodeWithEntriesRef<'_, T> {
        NodeWithEntriesRef {
            kind: self.kind,
            inner: &self.inner,
            entries: &self.entries,
        }
    }
}

impl<T> WriteBytes for HashedNodeWithEntries<T>
where
    T: WriteBytes + ObjectKindStr,
{
    fn write_bytes<W: Write>(&self, writer: &mut W) -> Result<(), GraphError> {
        self.as_node_with_entries_ref().write_bytes(writer)
    }
}

impl<T> VerifyHash for HashedNodeWithEntries<T>
where
    T: WriteBytes + ObjectKindStr,
{
    fn hash(&self) -> &Hash {
        &self.hash
    }
}

impl<T> From<HashedNodeWithEntries<T>> for (HashedNode<T>, Vec<NodeEntry>) {
    fn from(value: HashedNodeWithEntries<T>) -> Self {
        (
            HashedNode {
                kind: value.kind,
                hash: value.hash,
                inner: value.inner,
            },
            value.entries,
        )
    }
}

impl<T> From<HashedNode<T>> for NodeEntry
where
    T: NameStr,
{
    fn from(value: HashedNode<T>) -> Self {
        Self {
            kind: value.kind,
            hash: value.hash,
            name: value.inner.name().to_string(),
        }
    }
}

/// Reads a key/value formatted line from a reader and returns the value as a `String`.
///
/// # Errors
///
/// Returns an `Err` if:
///
/// - An I/O error occurs while reading from the reader
/// - If the line does not parse as a key/value line
/// - If the key name in the parsed line does not match the expected key name
pub fn read_key_value_line<R: BufRead>(
    reader: &mut R,
    key: impl AsRef<str>,
) -> Result<String, GraphError> {
    let mut line = String::new();
    reader.read_line(&mut line).map_err(GraphError::IoRead)?;
    let (line_key, line_value) = match line.trim_end().split_once('=') {
        Some((key, value)) => (key, value),
        None => return Err(GraphError::ParseLineKeyValueFormat(line)),
    };

    if line_key == key.as_ref() {
        Ok(line_value.to_string())
    } else {
        Err(GraphError::ParseLineExpectedKey(
            key.as_ref().to_string(),
            line_key.to_string(),
        ))
    }
}

/// Reads an empty line from a reader.
///
/// # Errors
///
/// Returns an `Err` if:
///
/// - An I/O error occurs while reading from the reader
/// - If the line is not empty as expected
fn read_empty_line<R: BufRead>(reader: &mut R) -> Result<(), GraphError> {
    let mut line = String::with_capacity(0);
    reader.read_line(&mut line).map_err(GraphError::IoRead)?;

    if line.trim_end().is_empty() {
        Ok(())
    } else {
        Err(GraphError::ParseLineBlank(line))
    }
}

/// Reads, parses, and return a `Vec` of [`NodeEntry`] items from a reader.
///
/// # Errors
///
/// Returns an `Err` if:
///
/// - An I/O error occurs while reading from the reader
/// - If the line can't be parsed as a node entry line
/// - If the node kind can't be parsed from the line
/// - If the hash value can't be parsed from the line
/// - If the name can't be parsed from the line
fn read_node_entry_lines<R: BufRead>(reader: &mut R) -> Result<Vec<NodeEntry>, GraphError> {
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(GraphError::IoRead)?;
        let mut parts: Vec<_> = line.rsplitn(3, ' ').collect();

        let kind = match parts.pop() {
            Some(s) => NodeKind::from_str(s).map_err(GraphError::parse)?,
            None => return Err(GraphError::parse_custom("missing kind field in entry line")),
        };
        let hash = match parts.pop() {
            Some(s) => Hash::from_str(s).map_err(GraphError::parse)?,
            None => return Err(GraphError::parse_custom("missing hash field in entry line")),
        };
        let name = match parts.pop() {
            Some(s) => s.to_string(),
            None => return Err(GraphError::parse_custom("missing name field in entry line")),
        };

        entries.push(NodeEntry { kind, hash, name });
    }

    Ok(entries)
}

/// Writes a node header to a writer.
///
/// # Errors
///
/// Returns `Err` if an I/O error occurs while writing to the writer
fn write_header_bytes<W: Write>(
    writer: &mut W,
    kind: NodeKind,
    object_kind: &str,
) -> Result<(), GraphError> {
    write_key_value_line(writer, KEY_VERSION_STR, VAL_VERSION_STR)?;
    write_key_value_line(writer, KEY_NODE_KIND_STR, kind.as_ref())?;
    write_key_value_line(writer, KEY_OBJECT_KIND_STR, object_kind)?;
    Ok(())
}

/// Writes a key/value formatted line to a writer with the given key and value.
///
/// # Errors
///
/// Returns `Err` if an I/O error occurs while writing to the writer
pub fn write_key_value_line<W: Write>(
    writer: &mut W,
    key: impl fmt::Display,
    value: impl fmt::Display,
) -> Result<(), GraphError> {
    write!(writer, "{key}={value}{NL}").map_err(GraphError::IoWrite)
}

/// Writes a separator/blank line to a writer.
///
/// # Errors
///
/// Returns `Err` if an I/O error occurs while writing to the writer
fn write_separator_bytes<W: Write>(writer: &mut W) -> Result<(), GraphError> {
    write!(writer, "{NL}").map_err(GraphError::IoWrite)
}