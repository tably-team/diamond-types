use std::fmt::{Display, Formatter};
use std::ops::Range;
/// Positional updates are a kind of operation (patch) which is larger than traversals but
/// retains temporal information. So, we know when each change happened relative to all other
/// changes.
///
/// Updates are made up of a series of insert / delete components, each at some position.

use smartstring::alias::{String as SmartString};
use rle::{HasLength, MergableSpan, SplitableSpanHelpers};
use OpKind::*;
use crate::unicount::{chars_to_bytes, count_chars};
use crate::list::internal_op::OperationInternal;
use crate::dtrange::DTRange;
use crate::rev_range::RangeRev;

#[cfg(feature = "serde")]
use serde_crate::{Deserialize, Serialize, Serializer};
#[cfg(feature = "serde")]
use serde_crate::ser::SerializeStruct;
#[cfg(feature = "serde")]
use crate::list::serde::FlattenSerializable;

/// So I might use this more broadly, for all edits. If so, move this out of OT.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(crate="serde_crate"))]
pub enum OpKind { Ins, Del }

impl Default for OpKind {
    fn default() -> Self { OpKind::Ins } // Arbitrary.
}

impl Display for OpKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Ins => f.write_str("Ins"),
            Del => f.write_str("Del")
        }
    }
}

/// So the span here is interesting. For inserts, this is the range of positions the inserted
/// characters *will have* after they've been inserted.
///
/// For deletes this is the range of characters in the document *being deleted*.
///
/// The `rev` field specifies if the items being inserted or deleted are doing so in reverse order.
/// For inserts "normal" mode means appending, reverse mode means prepending.
/// For deletes, normal mode means using the delete key. reverse mode means backspacing.
///
/// This has no effect on *what* is deleted. Only the resulting order of the operations. This is
/// totally unnecessary - we could just store extra entries with length 1 when modifying in other
/// orders. But it gives us way better compression for some data sets on disk. And this structure
/// is designed to match the on-disk file format.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize), serde(crate="serde_crate"))]
pub struct Operation {
    /// The range of items in the document being modified by this operation.
    // For now only backspaces are ever reversed.
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub loc: RangeRev,

    /// Is this operation an insert or a delete?
    pub kind: OpKind,

    /// What content is being inserted or deleted. This is optional for deletes. (And eventually
    /// inserts too, though that code path isn't exercised and may for now cause panics in some
    /// cases).
    pub content: Option<SmartString>,
}

impl HasLength for Operation {
    fn len(&self) -> usize {
        self.loc.len()
    }
}

#[cfg(feature = "serde")]
impl Serialize for Operation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        self.serialize_struct(serializer)
    }
}

#[cfg(feature = "serde")]
impl FlattenSerializable for Operation {
    fn struct_name() -> &'static str {
        "Operation"
    }

    fn num_serialized_fields() -> usize {
        2 + <RangeRev as FlattenSerializable>::num_serialized_fields()
    }

    fn serialize_fields<S>(&self, s: &mut S::SerializeStruct) -> Result<(), S::Error> where S: Serializer {
        s.serialize_field("kind", match self.kind {
            Ins => "Ins",
            Del => "Del",
        })?;
        self.loc.serialize_fields::<S>(s)?;
        s.serialize_field("content", &self.content)?;
        // if let Some(content) = self.content.as_ref() {
        //     s.serialize_field("content", content)?;
        // }
        Ok(())
    }
}

impl Operation {
    pub fn new_insert(pos: usize, content: &str) -> Self {
        let len = count_chars(content);
        Operation { loc: (pos..pos+len).into(), kind: Ins, content: Some(content.into()) }
    }

    pub fn new_delete(loc: Range<usize>) -> Self {
        Self::new_delete_dt(loc.into())
    }

    fn new_delete_dt(loc: RangeRev) -> Self {
        Operation { loc, kind: Del, content: None }
    }

    pub fn new_delete_with_content_range(loc: Range<usize>, content: SmartString) -> Self {
        debug_assert_eq!(count_chars(&content), loc.len());
        Operation { loc: loc.into(), kind: Del, content: Some(content) }
    }

    pub fn new_delete_with_content(pos: usize, content: SmartString) -> Self {
        let len = count_chars(&content);
        Self::new_delete_with_content_range(pos..pos+len, content)
    }

    pub fn range(&self) -> DTRange {
        self.loc.span
    }

    #[inline]
    pub fn start(&self) -> usize {
        self.loc.span.start
    }

    #[inline]
    pub fn end(&self) -> usize {
        self.loc.span.end
    }

    pub fn content_as_str(&self) -> Option<&str> {
        if let Some(c) = &self.content {
            Some(c.as_str())
        } else { None }
    }
}

impl SplitableSpanHelpers for Operation {
    fn truncate_h(&mut self, at: usize) -> Self {
        // let (self_span, other_span) = TimeSpanRev::split_op_span(self.span, self.tag, at);
        let span = self.loc.truncate_tagged_span(self.kind, at);

        let rem_content = self.content.as_mut().map(|c| {
            let byte_split = chars_to_bytes(c, at);
            c.split_off(byte_split)
        });

        // TODO: When we split items to a length of 1, consider clearing the reversed flag.
        // This doesn't do anything - but it feels polite.
        Self {
            loc: span,
            kind: self.kind,
            content: rem_content,
        }
    }
}

impl MergableSpan for Operation {
    fn can_append(&self, other: &Self) -> bool {
        if other.kind != self.kind || self.content.is_some() != other.content.is_some() { return false; }

        RangeRev::can_append_ops(self.kind, &self.loc, &other.loc)
    }

    fn append(&mut self, other: Self) {
        self.loc.append_ops(self.kind, other.loc);

        if let Some(c) = &mut self.content {
            c.push_str(&other.content.unwrap());
        }
    }

    // fn prepend(&mut self, mut other: Self) {
    //     // self.reversed = self.pos < other.pos || (other.pos == self.pos && self.tag == Ins);
    //     self.fwd = self.pos >= other.pos && (other.pos != self.pos || self.tag == Del);
    //
    //     if self.tag == Ins || self.fwd {
    //         self.pos = other.pos;
    //     }
    //     self.len += other.len;
    //
    //     if self.tag == Ins && self.content_known {
    //         other.content.push_str(&self.content);
    //         self.content = other.content;
    //     }
    // }
}

impl From<(OperationInternal, Option<&str>)> for Operation {
    fn from((op, content): (OperationInternal, Option<&str>)) -> Self {
        Operation {
            loc: op.loc,
            kind: op.kind,
            content: content.map(|str| str.into())
        }
    }
}

impl From<(&OperationInternal, Option<&str>)> for Operation {
    fn from((op, content): (&OperationInternal, Option<&str>)) -> Self {
        Operation {
            loc: op.loc,
            kind: op.kind,
            content: content.map(|str| str.into())
        }
    }
}


#[cfg(test)]
mod test {
    use rle::test_splitable_methods_valid;
    use super::*;

    #[test]
    fn test_backspace_merges() {
        // Make sure deletes collapse.
        let a = Operation {
            loc: (100..101).into(),
            kind: Del,
            content: Some("a".into()),
        };
        let b = Operation {
            loc: (99..100).into(),
            kind: Del,
            content: Some("b".into()),
        };
        assert!(a.can_append(&b));

        let mut merged = a.clone();
        merged.append(b.clone());
        // dbg!(&a);
        let expect = Operation {
            loc: RangeRev {
                span: (99..101).into(),
                fwd: false
            },
            kind: Del,
            content: Some("ab".into()),
        };
        assert_eq!(merged, expect);

        // And via prepend.
        let mut merged2 = b.clone();
        merged2.prepend(a.clone());
        dbg!(&merged2);
        assert_eq!(merged2, expect);
    }

    #[test]
    fn positional_component_splitable() {
        for fwd in [true, false] {
            for content in [Some("abcde".into()), None] {
                if fwd {
                    test_splitable_methods_valid(Operation {
                        loc: RangeRev {
                            span: (10..15).into(),
                            fwd
                        },
                        kind: Ins,
                        content: content.clone(),
                    });
                }

                test_splitable_methods_valid(Operation {
                    loc: RangeRev {
                        span: (10..15).into(),
                        fwd
                    },
                    kind: Del,
                    content
                });
            }
        }
    }
}