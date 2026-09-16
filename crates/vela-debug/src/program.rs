//! The story under debug: the compiled program, its entry point, and the sources its spans point
//! into.
//!
//! A compiled module is one program linked from every file (`LANGUAGE.md §6.1`), and its
//! instructions carry [`Span`]s whose `FileId`s come from the source map the *compiler* used. A
//! debugger needs that same map to answer the two questions a client asks constantly — *which
//! file and line is this instruction?* and *which byte offset is line N of that file?* — so it
//! holds a copy, taken once from the session that compiled the story. That is why `vela-span`'s
//! map is cloneable: the debugger's span and the compiler's are then the same span, and the
//! debugger cannot disagree with the build about which file a line is in.

use vela_bytecode::{Module, TypeId};
use vela_span::{FileId, SourceMap, Span};

/// A place in a source file: which file, and a 0-based line.
///
/// Zero-based because that is how the compiler counts, and `vela-span`'s `LineCol` is already in
/// that space. The one-based conversion DAP wants happens at the protocol edge and nowhere else,
/// so there is exactly one place it can be got wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position {
    /// The file.
    pub file: FileId,
    /// The line, 0-based.
    pub line: u32,
}

/// A compiled story, ready to be stepped.
pub struct Program {
    module: Module,
    entry: String,
    sources: SourceMap,
}

impl Program {
    /// A program over a compiled module, started at a label, with its sources.
    #[must_use]
    pub fn new(module: Module, entry: impl Into<String>, sources: SourceMap) -> Self {
        Self {
            module,
            entry: entry.into(),
            sources,
        }
    }

    /// The module being debugged.
    #[must_use]
    pub fn module(&self) -> &Module {
        &self.module
    }

    /// The label the story starts at.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }

    /// The sources the module's spans point into.
    #[must_use]
    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    /// Whether the module carries the debug info a line breakpoint needs.
    ///
    /// `Header::FLAG_DEBUG` (`BYTECODE.md §5`): a `--release` build clears it, and from then on
    /// the machine reports no span, so there is no line to break on. The debugger says so rather
    /// than accepting a breakpoint it could never honour.
    #[must_use]
    pub fn is_debuggable(&self) -> bool {
        self.module.header.has_debug()
    }

    /// The file and 0-based line a span starts at.
    #[must_use]
    pub fn position_of(&self, span: Span) -> Option<Position> {
        let file = self.sources.get(span.file())?;
        Some(Position {
            file: span.file(),
            line: file.line_col(span.start()).line,
        })
    }

    /// The byte offset a line begins at.
    #[must_use]
    pub fn offset_of(&self, file: FileId, line: u32) -> Option<u32> {
        Some(self.sources.get(file)?.line_range(line)?.start)
    }

    /// How many lines a file has.
    #[must_use]
    pub fn line_count(&self, file: FileId) -> Option<u32> {
        Some(self.sources.get(file)?.line_count())
    }

    /// A file's name, as the compiler named it.
    #[must_use]
    pub fn name_of(&self, file: FileId) -> Option<&str> {
        Some(self.sources.get(file)?.name())
    }

    /// The file a client's path names.
    ///
    /// A client sends an absolute path (`/home/me/story/src/main.vela`), and the compiler named
    /// the file relative to the project's `src` (`main.vela`). Matching on the *tail* is what
    /// bridges the two without either side having to know the other's convention, and it is the
    /// same rule an editor uses to associate a document with a project file. Backslashes are
    /// normalised so a Windows client matches too.
    #[must_use]
    pub fn file_for_path(&self, path: &str) -> Option<FileId> {
        let wanted = path.replace('\\', "/");
        let mut index = 0u32;
        loop {
            let file = FileId::from_raw(index);
            let source = self.sources.get(file)?;
            let name = source.name().replace('\\', "/");
            if wanted == name || wanted.ends_with(&format!("/{name}")) {
                return Some(file);
            }
            index += 1;
        }
    }

    /// The source text of a file.
    #[must_use]
    pub fn text_of(&self, file: FileId) -> Option<&str> {
        Some(self.sources.get(file)?.text())
    }

    /// A type as source: `int`, `list<str>`, `Route?`.
    ///
    /// Rendered by `vela-bytecode`, which already spells a type for its disassembly. Fetching it
    /// from there rather than writing a second renderer is what keeps a debugger's view of a
    /// value and the disassembly from ever disagreeing about how a type is written.
    #[must_use]
    pub fn type_source(&self, ty: TypeId) -> String {
        vela_bytecode::render_type(&self.module, ty)
    }
}
