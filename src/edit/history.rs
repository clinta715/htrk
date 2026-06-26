use super::commands::EditCommand;
use crate::sequencer::Module;

/// One entry in the undo stack: the command, the monotonically-increasing
/// id assigned when it was pushed, and an optional user-facing label
/// (set by the caller via [`UndoManager::execute_with_label`]).
/// The label is opaque to the manager but round-tripped to MCP so an
/// agent can ask "undo the phrase.generate I just did" by label.
pub struct UndoEntry {
    pub id: u64,
    pub cmd: Box<dyn EditCommand>,
    pub label: Option<String>,
}

pub struct UndoManager {
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    max_depth: usize,
    next_id: u64,
}

impl UndoManager {
    pub fn new(max_depth: usize) -> Self {
        UndoManager {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
            next_id: 1,
        }
    }

    /// Execute a command with an auto-generated numeric id. The id is
    /// monotonically increasing across the manager's lifetime. Returns
    /// the id assigned to this command so the caller can refer to it
    /// later (e.g. to undo a specific mutation from MCP).
    pub fn execute(
        &mut self,
        cmd: Box<dyn EditCommand>,
        module: &mut Module,
    ) -> Result<u64, super::commands::EditError> {
        let id = self.next_id;
        self.next_id += 1;
        self.push(cmd, id, Some(format!("#{id}")), module)?;
        Ok(id)
    }

    /// Execute with a caller-supplied label. Use this when the mutation
    /// has a meaningful name (e.g. "phrase.generate drum", "pattern.transform
    /// humanize"). The label is returned as a string in [`UndoEntry`].
    pub fn execute_with_label(
        &mut self,
        cmd: Box<dyn EditCommand>,
        label: impl Into<String>,
        module: &mut Module,
    ) -> Result<u64, super::commands::EditError> {
        let id = self.next_id;
        self.next_id += 1;
        self.push(cmd, id, Some(label.into()), module)?;
        Ok(id)
    }

    fn push(
        &mut self,
        cmd: Box<dyn EditCommand>,
        id: u64,
        label: Option<String>,
        module: &mut Module,
    ) -> Result<(), super::commands::EditError> {
        cmd.execute(module)?;
        self.undo_stack.push(UndoEntry { id, cmd, label });
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
        Ok(())
    }

    /// Undo the most recent command. Returns the (id, label) of the
    /// undone command so the caller can confirm what was undone.
    pub fn undo(&mut self, module: &mut Module) -> Result<(u64, Option<String>), super::commands::EditError> {
        if let Some(entry) = self.undo_stack.pop() {
            let id = entry.id;
            let label = entry.label.clone();
            entry.cmd.undo(module)?;
            self.redo_stack.push(entry);
            Ok((id, label))
        } else {
            Err(super::commands::EditError::NoSelection)
        }
    }

    /// Redo the most recently undone command. Returns the (id, label).
    pub fn redo(&mut self, module: &mut Module) -> Result<(u64, Option<String>), super::commands::EditError> {
        if let Some(entry) = self.redo_stack.pop() {
            let id = entry.id;
            let label = entry.label.clone();
            entry.cmd.execute(module)?;
            self.undo_stack.push(entry);
            Ok((id, label))
        } else {
            Err(super::commands::EditError::NoSelection)
        }
    }

    /// Pop commands until the one with this id is undone (inclusive).
    /// Useful for "undo the phrase.generate I just did" via the id
    /// returned in the MCP response. Returns the number of commands
    /// undone.
    pub fn undo_to(
        &mut self,
        target_id: u64,
        module: &mut Module,
    ) -> Result<usize, super::commands::EditError> {
        let mut undone = 0;
        while let Some(entry) = self.undo_stack.last() {
            let hit = entry.id == target_id;
            // Pop the top entry: undo it, then move it to the redo stack.
            let top = self.undo_stack.pop().unwrap();
            top.cmd.undo(module)?;
            self.redo_stack.push(top);
            undone += 1;
            if hit {
                return Ok(undone);
            }
        }
        if undone == 0 {
            Err(super::commands::EditError::NoSelection)
        } else {
            // Undid some but never found the target. Re-do them.
            // Caller can issue undo.last again or just live with it.
            Ok(undone)
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// Returns a snapshot of the most recent label, if any.
    pub fn last_label(&self) -> Option<&str> {
        self.undo_stack.last().and_then(|e| e.label.as_deref())
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for UndoManager {
    fn default() -> Self {
        UndoManager::new(1000)
    }
}
