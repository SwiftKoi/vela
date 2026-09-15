//! Lowering statements, including control flow.
//!
//! The shape of every construct here is the same four steps: open the blocks the construct
//! will need *before* sealing anything, seal the block being built, fill the new blocks,
//! then continue at the join. Opening early is what keeps block numbering stable, and
//! stable numbering is what makes a golden diff readable.

use vela_span::Span;
use vela_syntax::ReturnStmt;
use vela_syntax::{BinOp, ForStmt, IfStmt, Stmt, VarStmt, WhileStmt};
use vela_types::{Ty, lower as lower_type};

use crate::ir::{Const, Operand, Place, Slot, StmtKind, Terminator, Value};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// Lowers a sequence of statements into the block being built.
    pub(crate) fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    /// Lowers one statement.
    pub(crate) fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Var(decl) => self.var(decl),
            Stmt::Assign(assign) => self.assign(assign),
            Stmt::Expr(expr) => {
                // Evaluated for its effect, which is what a call in statement position is.
                self.expr(&expr.expr);
            }
            Stmt::Return(ret) => self.return_(ret),
            Stmt::If(stmt) => self.if_(stmt),
            Stmt::While(stmt) => self.while_stmt(stmt),
            Stmt::For(stmt) => self.for_(stmt),
            Stmt::Match(stmt) => self.match_(stmt),
            // Everything else is a story construct, and reaches the outside world.
            Stmt::Say(_)
            | Stmt::Menu(_)
            | Stmt::Jump(_)
            | Stmt::Call(_)
            | Stmt::Stage(_)
            | Stmt::With(_)
            | Stmt::Wait(_)
            | Stmt::Audio(_) => self.story(statement),
            Stmt::Error { .. } => {}
        }
    }

    /// `var x = value`.
    pub(crate) fn var(&mut self, decl: &VarStmt) {
        let ty = decl.ty.as_ref().map_or_else(
            || self.type_of(&decl.value),
            |written| lower_type(written, self.env),
        );
        let slot = self.declare(&decl.name, ty);
        self.expr_into(Place::Local(slot), &decl.value);
    }
    /// `return`, with or without a value.
    pub(crate) fn return_(&mut self, stmt: &ReturnStmt) {
        let value = stmt.value.as_ref().map(|value| self.expr(value));
        self.body.seal(Terminator::Return(value));
        // A return ends the path, so the rest of this body continues in a block that
        // nothing reaches. `dead_block` removes it.
        self.body.open();
    }

    /// `if` / `elif` / `else`.
    pub(crate) fn if_(&mut self, stmt: &IfStmt) {
        let entry = self.body.current();
        let join = self.body.open();

        let mut clauses: Vec<(&vela_syntax::Expr, &Vec<Stmt>)> =
            vec![(&stmt.condition, &stmt.then_body)];
        for clause in &stmt.elifs {
            clauses.push((&clause.condition, &clause.body));
        }

        self.body.start(entry);
        for (condition, body) in clauses {
            let condition = self.expr(condition);
            let head = self.body.current();
            let taken = self.body.open();
            let next = self.body.open();
            self.body.start(head);
            self.body.seal(Terminator::Branch {
                cond: condition,
                then_: taken,
                else_: next,
            });

            self.body.start(taken);
            self.statements(body);
            self.body.seal(Terminator::Goto(join));
            self.body.start(next);
        }

        if let Some(else_body) = &stmt.else_body {
            self.statements(else_body);
        }
        self.body.seal(Terminator::Goto(join));
        self.body.start(join);
    }

    /// `while condition`.
    pub(crate) fn while_stmt(&mut self, stmt: &WhileStmt) {
        let entry = self.body.current();
        let head = self.body.open();
        let body = self.body.open();
        let join = self.body.open();

        self.body.start(entry);
        self.body.seal(Terminator::Goto(head));

        self.body.start(head);
        let condition = self.expr(&stmt.condition);
        self.body.seal(Terminator::Branch {
            cond: condition,
            then_: body,
            else_: join,
        });

        self.body.start(body);
        self.statements(&stmt.body);
        self.body.seal(Terminator::Goto(head));

        self.body.start(join);
    }

    /// `for x in xs`, as an index loop.
    ///
    /// The language has no iterator protocol and MIR has no iteration instruction, so a
    /// `for` is a counted loop over the aggregate's length — which is what
    /// `BYTECODE.md §3.2`'s `ListLen` and `ListGet` exist for.
    pub(crate) fn for_(&mut self, stmt: &ForStmt) {
        let span = stmt.span;

        // Evaluated *before* the loop's blocks are opened: `open` moves the insertion
        // point, so a value built after it would be built in the wrong block and never
        // run before the loop that reads it.
        let iterable = self.expr(&stmt.iterable);
        let element = match self.type_of(&stmt.iterable) {
            Ty::List(element) => *element,
            _ => Ty::Unknown,
        };

        let entry = self.body.current();
        let head = self.body.open();
        let body = self.body.open();
        let step = self.body.open();
        let join = self.body.open();
        let index = self.temp(Ty::Int);

        self.body.start(entry);
        self.assign_constant(index, Const::Int(0), span);
        self.body.seal(Terminator::Goto(head));

        self.body.start(head);
        let test = self.loop_test(index, iterable, span);
        self.body.seal(Terminator::Branch {
            cond: test,
            then_: body,
            else_: join,
        });

        self.body.start(body);
        let binding = self.declare(&stmt.binding, element);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(binding),
                src: Operand::Index {
                    base: iterable,
                    index: Value::Slot(index),
                },
            },
            span,
        );
        self.statements(&stmt.body);
        self.body.seal(Terminator::Goto(step));

        self.body.start(step);
        let next = self.temp(Ty::Int);
        let one = self.constant(Const::Int(1));
        self.emit(
            StmtKind::Assign {
                dst: Place::Local(next),
                op: BinOp::Add,
                a: Value::Slot(index),
                b: one,
            },
            span,
        );
        self.emit(
            StmtKind::Load {
                dst: Place::Local(index),
                src: Operand::Value(Value::Slot(next)),
            },
            span,
        );
        self.body.seal(Terminator::Goto(head));

        self.body.start(join);
    }

    /// `index < len(aggregate)`.
    fn loop_test(&mut self, index: Slot, iterable: Value, span: Span) -> Value {
        let length = self.temp(Ty::Int);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(length),
                src: Operand::Len { base: iterable },
            },
            span,
        );
        let test = self.temp(Ty::Bool);
        self.emit(
            StmtKind::Assign {
                dst: Place::Local(test),
                op: BinOp::Lt,
                a: Value::Slot(index),
                b: Value::Slot(length),
            },
            span,
        );
        Value::Slot(test)
    }

    /// `slot = value`
    fn assign_constant(&mut self, slot: Slot, value: Const, span: Span) {
        let value = self.constant(value);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(slot),
                src: Operand::Value(value),
            },
            span,
        );
    }
}
