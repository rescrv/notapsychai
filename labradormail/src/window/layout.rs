//! Helpers for building window hierarchies.

use super::WindowId;
use super::WindowOrientation;
use super::WindowSize;
use super::WindowTree;
use super::WindowType;

/// Convenience builder for attaching windows to a parent.
pub struct LayoutBuilder<'a> {
    tree: &'a mut WindowTree,
    parent: WindowId,
}

impl<'a> LayoutBuilder<'a> {
    /// Creates a builder for attaching children under `parent`.
    pub fn new(tree: &'a mut WindowTree, parent: WindowId) -> Self {
        Self { tree, parent }
    }

    /// Adds an existing window as a child of the parent.
    pub fn add_existing(&mut self, child: WindowId) -> WindowId {
        self.tree.add_child(self.parent, child);
        child
    }

    /// Adds multiple existing windows as children of the parent.
    pub fn extend(&mut self, children: &[WindowId]) {
        for &child in children {
            self.tree.add_child(self.parent, child);
        }
    }

    /// Creates and attaches a new window.
    pub fn window(
        &mut self,
        window_type: WindowType,
        orient: WindowOrientation,
        size: WindowSize,
        cols: i16,
        rows: i16,
    ) -> WindowId {
        let child = self.tree.add_window(window_type, orient, size, cols, rows);
        self.tree.add_child(self.parent, child);
        child
    }

    /// Creates a container window, attaches it, and builds its children.
    pub fn container<F>(
        &mut self,
        window_type: WindowType,
        orient: WindowOrientation,
        size: WindowSize,
        cols: i16,
        rows: i16,
        build: F,
    ) -> WindowId
    where
        F: FnOnce(&mut LayoutBuilder<'_>),
    {
        let child = self.window(window_type, orient, size, cols, rows);
        let mut builder = LayoutBuilder::new(self.tree, child);
        build(&mut builder);
        child
    }

    /// Creates a vertical container and builds its children.
    pub fn column<F>(
        &mut self,
        window_type: WindowType,
        size: WindowSize,
        cols: i16,
        rows: i16,
        build: F,
    ) -> WindowId
    where
        F: FnOnce(&mut LayoutBuilder<'_>),
    {
        self.container(
            window_type,
            WindowOrientation::Vertical,
            size,
            cols,
            rows,
            build,
        )
    }

    /// Creates a horizontal container and builds its children.
    pub fn row<F>(
        &mut self,
        window_type: WindowType,
        size: WindowSize,
        cols: i16,
        rows: i16,
        build: F,
    ) -> WindowId
    where
        F: FnOnce(&mut LayoutBuilder<'_>),
    {
        self.container(
            window_type,
            WindowOrientation::Horizontal,
            size,
            cols,
            rows,
            build,
        )
    }

    /// Creates a vertical container window of type Container.
    pub fn column_container<F>(
        &mut self,
        size: WindowSize,
        cols: i16,
        rows: i16,
        build: F,
    ) -> WindowId
    where
        F: FnOnce(&mut LayoutBuilder<'_>),
    {
        self.column(WindowType::Container, size, cols, rows, build)
    }

    /// Creates a horizontal container window of type Container.
    pub fn row_container<F>(&mut self, size: WindowSize, cols: i16, rows: i16, build: F) -> WindowId
    where
        F: FnOnce(&mut LayoutBuilder<'_>),
    {
        self.row(WindowType::Container, size, cols, rows, build)
    }

    /// Creates and attaches a new dialog window.
    pub fn dialog(&mut self, window_type: WindowType) -> WindowId {
        let child = self.tree.add_dialog(window_type);
        self.tree.add_child(self.parent, child);
        child
    }
}
