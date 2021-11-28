use jumprope::JumpRope;
use crate::list::{Branch, OpLog, Time};
use smallvec::smallvec;
use smartstring::SmartString;
use rle::HasLength;
use crate::list::operation::InsDelTag::*;
use crate::list::operation::Operation;
use crate::localtime::TimeSpan;
use crate::ROOT_TIME;

impl Branch {
    pub fn new() -> Self {
        Self {
            frontier: smallvec![ROOT_TIME],
            content: JumpRope::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.content.len_chars()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Apply a single operation. This method does not update the frontier - that is left as an
    /// exercise for the caller.
    pub(crate) fn apply_1(&mut self, op: &Operation) {
        let pos = op.pos;

        match op.tag {
            Ins => {
                assert!(op.content_known);
                self.content.insert(pos, &op.content);
            }

            Del => {
                self.content.remove(pos..pos + op.len());
            }
        }
    }

    /// Apply a set of operations. Does not update frontier.
    #[allow(unused)]
    pub(crate) fn apply(&mut self, ops: &[Operation]) {
        for c in ops {
            self.apply_1(c);
        }
    }

    pub(crate) fn apply_range_from(&mut self, ops: &OpLog, range: TimeSpan) {
        for op in ops.iter_range(range) {
            self.apply_1(&op.1);
        }
    }

    pub fn merge(&mut self, ops: &OpLog, merge_frontier: &[Time]) {
        self.merge_changes_m2(ops, merge_frontier);
    }

    pub fn make_delete_op(&self, pos: usize, del_span: usize) -> Operation {
        assert!(pos + del_span <= self.content.len_chars());
        let mut s = SmartString::new();
        s.extend(self.content.slice_chars(pos..pos+del_span));
        Operation::new_delete_with_content(pos, s)
    }
}

impl Default for Branch {
    fn default() -> Self {
        Self::new()
    }
}
