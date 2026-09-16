//! The server's state, and the three things it does without looking at a request body: start, run, and
//! say what it can do.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use serde_json::{Value, json};
use vela_compile::Session;

use crate::docs::Reference;
use crate::transport;
use crate::uri;

/// Builds a session for the workspace rooted at a path.
///
/// The command line implements this with the loader `vela check` uses, which is what keeps the editor
/// and the command in agreement: one function decides what a project's files are.
type Loader = Box<dyn Fn(&str) -> Session>;

/// One language server.
pub struct Server {
    /// How a workspace becomes a session.
    pub(super) load: Loader,
    /// The workspace: its files, and everything the compiler has memoized about them.
    pub(super) session: Session,
    /// The project root, as the client named it or as the process was started.
    pub(super) root: String,
    /// The directory file *names* are relative to: the root's `src`, when it has one.
    pub(super) source: String,
    /// Where the workspace's generated reference is, for hover to link into.
    pub(super) reference: Reference,
    /// Open documents: URI to the name the session knows the file by.
    pub(super) open: BTreeMap<String, String>,
}

impl Server {
    /// A server for the workspace at `root`, loaded by `load`.
    #[must_use]
    pub fn new(root: impl Into<String>, load: impl Fn(&str) -> Session + 'static) -> Self {
        let root = root.into();
        let session = load(&root);
        let source = source_root(&root);
        let reference = Reference::new(&root);

        Self {
            load: Box::new(load),
            session,
            root,
            source,
            reference,
            open: BTreeMap::new(),
        }
    }

    /// Answers messages until the client exits or the stream ends.
    ///
    /// # Errors
    ///
    /// Fails on a transport error, which is not recoverable: a stream whose framing is broken has no
    /// next message to resynchronise to.
    pub fn serve(
        &mut self,
        input: &mut impl BufRead,
        // `dyn` rather than `impl`: the caller is a subcommand, and a subcommand is handed
        // `&mut dyn Write` by the driver. Generic here would only move the same coercion to the call
        // site, and there is nothing to inline.
        output: &mut dyn Write,
    ) -> std::io::Result<()> {
        while let Some(message) = transport::read(input)? {
            if !self.handle(&message, output)? {
                break;
            }
        }
        Ok(())
    }

    /// What this server can do.
    pub(super) fn capabilities(&self) -> Value {
        json!({
            "capabilities": {
                // Full sync: a change carries the whole document. Incremental sync is an optimisation
                // for large files, and building it before the features that need positions exist would
                // be work in the wrong order.
                "textDocumentSync": 1,
                // Stated rather than left to the default, because it is a claim about every range this
                // server ever sends (`crate::position`).
                "positionEncoding": "utf-16",
                // What the symbol index and the checker can answer: where a name is declared, every
                // place it is written, a rename of it, what it is, and what could be typed next.
                "definitionProvider": true,
                "referencesProvider": true,
                "renameProvider": true,
                "hoverProvider": true,
                // No trigger characters: a `.` after a name would normally mean "the members of this
                // value", and that list is not built yet. Declaring it would make an editor ask for
                // members and be told there are none, which reads as a bug rather than as a gap.
                "completionProvider": {},
            },
            "serverInfo": { "name": "vela-lsp", "version": env!("CARGO_PKG_VERSION") },
        })
    }

    /// Adopts the root the client names, reloading when it is not the one we started with.
    ///
    /// An editor launched anywhere can open a project anywhere, so the root the client sends is the one
    /// that counts — but reloading on every `initialize` would throw away the memoized results of a
    /// server that was started in the right place, so it happens only when the answer differs.
    pub(super) fn adopt_root(&mut self, message: &Value) {
        let named = message["params"]["rootUri"]
            .as_str()
            .and_then(uri::path)
            .or_else(|| {
                message["params"]["rootPath"]
                    .as_str()
                    .map(ToString::to_string)
            });

        if let Some(root) = named
            && root != self.root
        {
            self.session = (self.load)(&root);
            self.source = source_root(&root);
            self.reference = Reference::new(&root);
            self.root = root;
            self.open.clear();
        }
    }
}

/// The directory a file's *name* is relative to: the project's `src`, when it has one.
///
/// This is the rule the command line uses — `vela check` walks `src/` and names a file by its path
/// inside it — and matching it is not a detail: a session keys a file by name, and a module's dotted
/// name is derived from it (`LANGUAGE.md §6`). Naming a file `src/main.vela` where the command line
/// says `main.vela` gives the module `src.main`, so `use main` in another file resolves in one and not
/// the other, and the editor quietly disagrees with the build about the whole project's structure.
fn source_root(root: &str) -> String {
    let src = std::path::Path::new(root).join("src");
    if src.is_dir() {
        src.display().to_string()
    } else {
        root.to_string()
    }
}
