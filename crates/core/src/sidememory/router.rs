use thiserror::Error;

use crate::json_schema::ir::{SchemaIr, SchemaNodeId};

use super::journal::{JournalError, RouterJournal, RouterUndo};
use super::limits::ResourceError;
use super::scope::FrameId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParentTarget {
    Root,
    Array(FrameId),
    Object(FrameId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeFrame {
    Array {
        frame: FrameId,
        schema_node: SchemaNodeId,
        item_node: SchemaNodeId,
        parent: ParentTarget,
    },
    Object {
        frame: FrameId,
        schema_node: SchemaNodeId,
        pending_property: Option<SchemaNodeId>,
        parent: ParentTarget,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticRouter {
    stack: Vec<RuntimeFrame>,
    root_node: SchemaNodeId,
    pending_node: SchemaNodeId,
    root_complete: bool,
}

impl SemanticRouter {
    pub fn new(root_node: SchemaNodeId) -> Self {
        Self {
            stack: Vec::new(),
            root_node,
            pending_node: root_node,
            root_complete: false,
        }
    }

    pub fn expected_node(&self) -> SchemaNodeId {
        self.pending_node
    }

    pub fn open_array(
        &mut self,
        frame: FrameId,
        ir: &SchemaIr,
    ) -> Result<SchemaNodeId, RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.open_array_journaled(frame, ir, &mut journal)
    }

    pub fn open_array_journaled(
        &mut self,
        frame: FrameId,
        ir: &SchemaIr,
        journal: &mut RouterJournal,
    ) -> Result<SchemaNodeId, RouterError> {
        let schema_node = self.pending_node;
        let item_node = ir
            .node(schema_node)
            .and_then(|node| node.array.as_ref())
            .map(|array| array.items)
            .ok_or(RouterError::ExpectedArray(schema_node))?;
        let parent = self.parent_target();
        self.stack
            .try_reserve_exact(1)
            .map_err(|_| ResourceError::Allocation {
                context: "semantic router frames",
                requested: 1,
            })?;
        journal.preflight(1)?;
        journal.push_preflighted(RouterUndo::PopOpenedFrame {
            previous_pending: self.pending_node,
        });
        self.stack.push(RuntimeFrame::Array {
            frame,
            schema_node,
            item_node,
            parent,
        });
        self.pending_node = item_node;
        Ok(schema_node)
    }

    pub fn open_object(
        &mut self,
        frame: FrameId,
        ir: &SchemaIr,
    ) -> Result<SchemaNodeId, RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.open_object_journaled(frame, ir, &mut journal)
    }

    pub fn open_object_journaled(
        &mut self,
        frame: FrameId,
        ir: &SchemaIr,
        journal: &mut RouterJournal,
    ) -> Result<SchemaNodeId, RouterError> {
        let schema_node = self.pending_node;
        if ir
            .node(schema_node)
            .and_then(|node| node.object.as_ref())
            .is_none()
        {
            return Err(RouterError::ExpectedObject(schema_node));
        }
        let parent = self.parent_target();
        self.stack
            .try_reserve_exact(1)
            .map_err(|_| ResourceError::Allocation {
                context: "semantic router frames",
                requested: 1,
            })?;
        journal.preflight(1)?;
        journal.push_preflighted(RouterUndo::PopOpenedFrame {
            previous_pending: self.pending_node,
        });
        self.stack.push(RuntimeFrame::Object {
            frame,
            schema_node,
            pending_property: None,
            parent,
        });
        Ok(schema_node)
    }

    pub fn set_pending_property(
        &mut self,
        frame: FrameId,
        key: &[u8],
        ir: &SchemaIr,
    ) -> Result<SchemaNodeId, RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.set_pending_property_journaled(frame, key, ir, &mut journal)
    }

    pub fn set_pending_property_journaled(
        &mut self,
        frame: FrameId,
        key: &[u8],
        ir: &SchemaIr,
        journal: &mut RouterJournal,
    ) -> Result<SchemaNodeId, RouterError> {
        let key = std::str::from_utf8(key).map_err(|_| RouterError::InvalidPropertyUtf8)?;
        let frame_index = self
            .stack
            .iter()
            .rposition(|runtime| runtime.id() == frame)
            .ok_or(RouterError::UnknownFrame(frame))?;
        let RuntimeFrame::Object { schema_node, .. } = &self.stack[frame_index] else {
            return Err(RouterError::ExpectedObjectFrame(frame));
        };
        let child = ir
            .node(*schema_node)
            .and_then(|node| node.object.as_ref())
            .and_then(|object| {
                object
                    .properties
                    .iter()
                    .find_map(|(name, node)| (name.as_ref() == key).then_some(*node))
            })
            .ok_or_else(|| RouterError::UnknownProperty(key.into()))?;
        let previous_property = match &self.stack[frame_index] {
            RuntimeFrame::Object {
                pending_property, ..
            } => *pending_property,
            _ => return Err(RouterError::ExpectedObjectFrame(frame)),
        };
        journal.preflight(1)?;
        journal.push_preflighted(RouterUndo::RestorePending {
            frame_index,
            previous_property,
            previous_pending: self.pending_node,
        });
        let RuntimeFrame::Object {
            pending_property, ..
        } = &mut self.stack[frame_index]
        else {
            return Err(RouterError::ExpectedObjectFrame(frame));
        };
        *pending_property = Some(child);
        self.pending_node = child;
        Ok(child)
    }

    pub fn complete_array_item(&mut self, frame: FrameId) -> Result<(), RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.complete_array_item_journaled(frame, &mut journal)
    }

    pub fn complete_array_item_journaled(
        &mut self,
        frame: FrameId,
        journal: &mut RouterJournal,
    ) -> Result<(), RouterError> {
        let frame_index = self
            .stack
            .iter()
            .rposition(|runtime| runtime.id() == frame)
            .ok_or(RouterError::UnknownFrame(frame))?;
        let RuntimeFrame::Array { item_node, .. } = &self.stack[frame_index] else {
            return Err(RouterError::ExpectedArrayFrame(frame));
        };
        journal.preflight(1)?;
        journal.push_preflighted(RouterUndo::RestorePending {
            frame_index,
            previous_property: None,
            previous_pending: self.pending_node,
        });
        self.pending_node = *item_node;
        Ok(())
    }

    pub fn complete_property(&mut self, frame: FrameId) -> Result<(), RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.complete_property_journaled(frame, &mut journal)
    }

    pub fn complete_property_journaled(
        &mut self,
        frame: FrameId,
        journal: &mut RouterJournal,
    ) -> Result<(), RouterError> {
        let frame_index = self
            .stack
            .iter()
            .rposition(|runtime| runtime.id() == frame)
            .ok_or(RouterError::UnknownFrame(frame))?;
        let previous_property = match &self.stack[frame_index] {
            RuntimeFrame::Object {
                pending_property, ..
            } => *pending_property,
            _ => return Err(RouterError::ExpectedObjectFrame(frame)),
        };
        if previous_property.is_none() {
            return Err(RouterError::MissingPendingProperty(frame));
        }
        journal.preflight(1)?;
        journal.push_preflighted(RouterUndo::RestorePending {
            frame_index,
            previous_property,
            previous_pending: self.pending_node,
        });
        let runtime = &mut self.stack[frame_index];
        let RuntimeFrame::Object {
            schema_node,
            pending_property,
            ..
        } = runtime
        else {
            return Err(RouterError::ExpectedObjectFrame(frame));
        };
        if pending_property.take().is_none() {
            return Err(RouterError::MissingPendingProperty(frame));
        }
        self.pending_node = *schema_node;
        Ok(())
    }

    pub fn close(&mut self, frame: FrameId) -> Result<(), RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.close_journaled(frame, &mut journal)
    }

    pub fn close_journaled(
        &mut self,
        frame: FrameId,
        journal: &mut RouterJournal,
    ) -> Result<(), RouterError> {
        let expected = self
            .stack
            .last()
            .ok_or(RouterError::UnknownFrame(frame))?
            .id();
        if expected != frame {
            return Err(RouterError::CloseOrder {
                expected,
                actual: frame,
            });
        }
        journal.preflight(1)?;
        let runtime = self.stack.pop().ok_or(RouterError::UnknownFrame(frame))?;
        journal.push_preflighted(RouterUndo::RestoreClosedFrame {
            frame: runtime,
            previous_pending: self.pending_node,
        });
        self.pending_node = match self.stack.last() {
            Some(RuntimeFrame::Array { item_node, .. }) => *item_node,
            Some(RuntimeFrame::Object {
                pending_property: Some(node),
                ..
            }) => *node,
            Some(RuntimeFrame::Object { schema_node, .. }) => *schema_node,
            None => self.root_node,
        };
        Ok(())
    }

    pub fn array_schema_node(&self, frame: FrameId) -> Result<SchemaNodeId, RouterError> {
        match self
            .stack
            .iter()
            .rev()
            .find(|runtime| runtime.id() == frame)
        {
            Some(RuntimeFrame::Array { schema_node, .. }) => Ok(*schema_node),
            Some(_) => Err(RouterError::ExpectedArrayFrame(frame)),
            None => Err(RouterError::UnknownFrame(frame)),
        }
    }

    pub fn complete_root(&mut self) -> Result<(), RouterError> {
        let mut journal = RouterJournal::new(usize::MAX);
        self.complete_root_journaled(&mut journal)
    }

    pub fn complete_root_journaled(
        &mut self,
        journal: &mut RouterJournal,
    ) -> Result<(), RouterError> {
        if !self.stack.is_empty() {
            return Err(RouterError::RootWithOpenFrames);
        }
        journal.preflight(1)?;
        journal.push_preflighted(RouterUndo::RestoreRootComplete(self.root_complete));
        self.root_complete = true;
        Ok(())
    }

    pub fn is_root_complete(&self) -> bool {
        self.root_complete
    }

    pub fn frames(&self) -> &[RuntimeFrame] {
        &self.stack
    }

    fn parent_target(&self) -> ParentTarget {
        match self.stack.last() {
            Some(RuntimeFrame::Array { frame, .. }) => ParentTarget::Array(*frame),
            Some(RuntimeFrame::Object { frame, .. }) => ParentTarget::Object(*frame),
            None => ParentTarget::Root,
        }
    }

    pub(crate) fn undo(&mut self, undo: RouterUndo) -> Result<(), JournalError> {
        match undo {
            RouterUndo::PopOpenedFrame { previous_pending } => {
                self.stack.pop().ok_or(JournalError::Invariant)?;
                self.pending_node = previous_pending;
            }
            RouterUndo::RestoreClosedFrame {
                frame,
                previous_pending,
            } => {
                self.stack.push(frame);
                self.pending_node = previous_pending;
            }
            RouterUndo::RestorePending {
                frame_index,
                previous_property,
                previous_pending,
            } => {
                let frame = self
                    .stack
                    .get_mut(frame_index)
                    .ok_or(JournalError::Invariant)?;
                if let RuntimeFrame::Object {
                    pending_property, ..
                } = frame
                {
                    *pending_property = previous_property;
                } else if previous_property.is_some() {
                    return Err(JournalError::Invariant);
                }
                self.pending_node = previous_pending;
            }
            RouterUndo::RestoreRootComplete(value) => self.root_complete = value,
        }
        Ok(())
    }
}

impl RuntimeFrame {
    pub fn id(&self) -> FrameId {
        match self {
            Self::Array { frame, .. } | Self::Object { frame, .. } => *frame,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RouterError {
    #[error("schema node {0:?} does not describe an array")]
    ExpectedArray(SchemaNodeId),
    #[error("schema node {0:?} does not describe an object")]
    ExpectedObject(SchemaNodeId),
    #[error("unknown runtime frame {0:?}")]
    UnknownFrame(FrameId),
    #[error("runtime frame {0:?} is not an array")]
    ExpectedArrayFrame(FrameId),
    #[error("runtime frame {0:?} is not an object")]
    ExpectedObjectFrame(FrameId),
    #[error("object frame {0:?} has no pending property")]
    MissingPendingProperty(FrameId),
    #[error("property key is not UTF-8")]
    InvalidPropertyUtf8,
    #[error("property {0} has no compiled schema node")]
    UnknownProperty(Box<str>),
    #[error("runtime frame close order mismatch: expected {expected:?}, got {actual:?}")]
    CloseOrder { expected: FrameId, actual: FrameId },
    #[error("root completed while runtime frames remain open")]
    RootWithOpenFrames,
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
}
