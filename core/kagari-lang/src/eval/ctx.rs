//! What the const-evaluation driver shares: the database, std items and signatures, and
//! the bodies it reads.

use crate::db::Db;
use crate::hir::{Body, body_of};
use crate::infer::{InferenceResult, inference};
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::resolve::{BodyResolution, body_resolution};
use crate::signatures::ProgramSignatures;

use super::machine::Machine;
use super::std_items::StdTable;

/// What every evaluation of one program shares: the database, std items and signatures.
pub(crate) struct Ctx<'db> {
    pub db: &'db dyn Db,
    pub program: Program,
    pub std: &'db StdTable,
    pub sigs: &'db ProgramSignatures,
}

/// The body of an item with everything evaluation reads.
#[derive(Clone, Copy)]
pub(crate) struct BodyRef<'db> {
    pub item: ItemId,
    pub body: &'db Body,
    pub res: &'db BodyResolution,
    pub infer: &'db InferenceResult,
}

impl<'db> Ctx<'db> {
    pub fn new(
        db: &'db dyn Db,
        program: Program,
        std: &'db StdTable,
        sigs: &'db ProgramSignatures,
    ) -> Ctx<'db> {
        Ctx {
            db,
            program,
            std,
            sigs,
        }
    }

    pub fn body(&self, item: ItemId) -> Option<BodyRef<'db>> {
        let body = body_of(self.db, item)?;
        let res = body_resolution(self.db, self.program, item).body.as_ref()?;
        let infer = inference(self.db, self.program, item)?;
        Some(BodyRef {
            item,
            body,
            res,
            infer,
        })
    }

    /// A fresh machine for this program.
    pub fn machine(&self) -> Machine<'db> {
        Machine::new(self.db, self.program, self.std)
    }
}
