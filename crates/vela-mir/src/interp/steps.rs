//! Executing one statement.
//!
//! Dispatched by *family* rather than by variant: arithmetic, copies, calls, aggregate
//! construction, and payload reads are five different jobs that happen to arrive through
//! one function, and each is easier to check against `LANGUAGE.md §4` on its own.

use vela_world::Value as WorldValue;

use super::machine::{Answer, Machine, Step};
use super::ops;
use crate::ir::{Callee, FuncRef, Place, StmtKind};

impl Machine<'_> {
    /// Executes one statement.
    ///
    /// Dispatched by *family* rather than by variant: arithmetic, copies, calls, aggregate
    /// construction, and payload reads are four different jobs that happen to arrive
    /// through one function.
    pub(super) fn statement(&mut self, stmt: &StmtKind) -> Result<Step, String> {
        match stmt {
            StmtKind::Assign { .. } | StmtKind::AssignUn { .. } => self.arithmetic(stmt),
            StmtKind::Load { .. } | StmtKind::Cmd { .. } => self.simple(stmt),
            StmtKind::Call { .. } => self.call_statement(stmt),
            StmtKind::ListNew { .. }
            | StmtKind::MapNew { .. }
            | StmtKind::StructNew { .. }
            | StmtKind::EnumNew { .. } => self.aggregate(stmt),
            StmtKind::EnumField { .. } | StmtKind::IsNone { .. } | StmtKind::Unwrap { .. } => {
                self.payload(stmt)
            }
        }
    }

    /// Arithmetic and prefix operators.
    fn arithmetic(&mut self, stmt: &StmtKind) -> Result<Step, String> {
        match stmt {
            StmtKind::Assign { dst, op, a, b } => {
                let left = self.read(*a);
                let right = self.read(*b);
                let value = ops::binary(*op, left, right)
                    .ok_or_else(|| format!("`{op:?}` is not defined for these operands"))?;
                self.write(dst, value);
            }
            StmtKind::AssignUn { dst, op, a } => {
                let operand = self.read(*a);
                let value = ops::unary(*op, operand)
                    .ok_or_else(|| format!("`{op:?}` is not defined for this operand"))?;
                self.write(dst, value);
            }
            _ => return Err("not an arithmetic statement".to_string()),
        }
        Ok(Step::Continue)
    }

    /// A copy, or a command being built for the next suspension.
    fn simple(&mut self, stmt: &StmtKind) -> Result<Step, String> {
        match stmt {
            StmtKind::Load { dst, src } => {
                let value = self.operand(src)?;
                self.write(dst, value);
            }
            StmtKind::Cmd { args, .. } => {
                self.pending = args.iter().map(|arg| self.read(*arg)).collect();
            }
            _ => return Err("not a simple statement".to_string()),
        }
        Ok(Step::Continue)
    }

    /// A call: a conversion, a declared function, or a function held in a variable.
    fn call_statement(&mut self, stmt: &StmtKind) -> Result<Step, String> {
        let StmtKind::Call { dst, callee, args } = stmt else {
            return Err("not a call".to_string());
        };
        let values: Vec<WorldValue> = args.iter().map(|arg| self.read(*arg)).collect();

        match callee {
            // A host capability, which this interpreter has none of. Saying so beats
            // guessing: the reference interpreter exists to give MIR a meaning, and an
            // effect's meaning *is* whatever the host decides.
            Callee::Direct(FuncRef::Effect(index)) => {
                let name = self
                    .module
                    .effects
                    .get(*index as usize)
                    .map_or_else(|| format!("#{index}"), |effect| effect.name.clone());
                Err(format!(
                    "`{name}` is a host effect, and this interpreter has no host"
                ))
            }
            Callee::Direct(FuncRef::Builtin(builtin)) => {
                let value = ops::convert(
                    builtin.as_str(),
                    values.into_iter().next().unwrap_or(WorldValue::None),
                )
                .ok_or_else(|| format!("`{}` cannot convert this value", builtin.as_str()))?;
                if let Some(dst) = dst {
                    self.write(dst, value);
                }
                Ok(Step::Continue)
            }
            Callee::Direct(FuncRef::Defined(index)) => {
                self.enter_function(*index, values, dst)?;
                Ok(Step::Entered)
            }
            Callee::Indirect(slot) => {
                let target = self.read(*slot);
                let WorldValue::Function(name) = target else {
                    return Err("a call target is not a function".to_string());
                };
                let index = self
                    .module
                    .fns
                    .iter()
                    .position(|body| body.name.as_str() == name)
                    .ok_or_else(|| format!("no function `{name}`"))?;
                let index = u32::try_from(index).map_err(|_| "too many functions")?;
                self.enter_function(index, values, dst)?;
                Ok(Step::Entered)
            }
        }
    }

    /// Building a list, a map, a struct, or a variant.
    fn aggregate(&mut self, stmt: &StmtKind) -> Result<Step, String> {
        match stmt {
            StmtKind::ListNew { dst, items } => {
                let items: Vec<WorldValue> = items.iter().map(|item| self.read(*item)).collect();
                self.write(dst, WorldValue::List(items));
            }
            StmtKind::MapNew { dst, entries } => {
                let mut map = std::collections::BTreeMap::new();
                for (key, value) in entries {
                    let key = vela_world::Key::of(&self.read(*key)).ok_or_else(|| {
                        "a map key must be an int, a string, or a bool".to_string()
                    })?;
                    map.insert(key, self.read(*value));
                }
                self.write(dst, WorldValue::Map(map));
            }
            StmtKind::StructNew { dst, name, fields } => {
                let fields: Vec<(String, WorldValue)> = fields
                    .iter()
                    .map(|(field, value)| (field.clone(), self.read(*value)))
                    .collect();
                self.write(
                    dst,
                    WorldValue::Struct {
                        name: name.clone(),
                        fields,
                    },
                );
            }
            StmtKind::EnumNew {
                dst,
                enum_name,
                variant,
                args,
            } => {
                let args: Vec<WorldValue> = args.iter().map(|arg| self.read(*arg)).collect();
                self.write(
                    dst,
                    WorldValue::Enum {
                        name: enum_name.clone(),
                        variant: variant.clone(),
                        fields: args,
                    },
                );
            }
            _ => return Err("not an aggregate construction".to_string()),
        }
        Ok(Step::Continue)
    }

    /// Reading a variant's payload, or asking whether an optional has one.
    fn payload(&mut self, stmt: &StmtKind) -> Result<Step, String> {
        match stmt {
            StmtKind::EnumField { dst, base, index } => {
                let base = self.read(*base);
                let WorldValue::Enum { fields, .. } = base else {
                    return Err("a payload read needs an enum value".to_string());
                };
                let value = fields
                    .get(*index as usize)
                    .cloned()
                    .unwrap_or(WorldValue::None);
                self.write(dst, value);
            }
            StmtKind::IsNone { dst, base } => {
                let value = self.read(*base);
                self.write(dst, WorldValue::Bool(value == WorldValue::None));
            }
            StmtKind::Unwrap { dst, base } => {
                let value = self.read(*base);
                if value == WorldValue::None {
                    return Err("`!` applied to `none`".to_string());
                }
                self.write(dst, value);
            }
            _ => return Err("not a payload read".to_string()),
        }
        Ok(Step::Continue)
    }

    /// Suspends, handing the host a command.
    pub(super) fn suspend(&mut self, site: &crate::ir::YieldSite) -> Result<Step, String> {
        let args = std::mem::take(&mut self.pending);
        let command = ops::command(site.command, &args).ok_or_else(|| {
            format!(
                "`{}` was built with the wrong arguments",
                site.command.as_str()
            )
        })?;
        self.commands.push(command);

        let answer = self
            .answers
            .get(self.cursor)
            .copied()
            .unwrap_or(Answer::Ack);
        self.cursor += 1;

        if let Some(result) = site.result {
            let chosen = match answer {
                Answer::Choice(index) => index,
                // A menu the host did not answer is a gap in a scripted run rather than a
                // fault in the story, so the first choice is taken.
                Answer::Ack => 0,
            };
            self.write(
                &Place::Local(result),
                WorldValue::Int(i64::try_from(chosen).unwrap_or(0)),
            );
        }

        self.jump_to(site.resume);
        Ok(Step::Continue)
    }
}
