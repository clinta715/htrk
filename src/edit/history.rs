use super::commands::EditCommand;
use crate::sequencer::Module;

pub struct UndoManager {
    undo_stack: Vec<Box<dyn EditCommand>>,
    redo_stack: Vec<Box<dyn EditCommand>>,
    max_depth: usize,
}

impl UndoManager {
    pub fn new(max_depth: usize) -> Self {
        UndoManager {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    pub fn execute(
        &mut self,
        cmd: Box<dyn EditCommand>,
        module: &mut Module,
    ) -> Result<(), super::commands::EditError> {
        cmd.execute(module)?;
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
        Ok(())
    }

    pub fn undo(&mut self, module: &mut Module) -> Result<(), super::commands::EditError> {
        if let Some(cmd) = self.undo_stack.pop() {
            cmd.undo(module)?;
            self.redo_stack.push(cmd);
            Ok(())
        } else {
            Err(super::commands::EditError::NoSelection)
        }
    }

    pub fn redo(&mut self, module: &mut Module) -> Result<(), super::commands::EditError> {
        if let Some(cmd) = self.redo_stack.pop() {
            cmd.execute(module)?;
            self.undo_stack.push(cmd);
            Ok(())
        } else {
            Err(super::commands::EditError::NoSelection)
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
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
