use super::{Page, ToksApp};

#[derive(Debug, Clone)]
pub(super) struct PageNavigation {
    visited: Vec<Page>,
    cursor: usize,
}

impl Default for PageNavigation {
    fn default() -> Self {
        Self {
            visited: vec![Page::Overview],
            cursor: 0,
        }
    }
}

impl PageNavigation {
    pub(super) fn current(&self) -> Page {
        self.visited[self.cursor]
    }

    pub(super) fn visit(&mut self, page: Page) -> bool {
        if self.current() == page {
            return false;
        }
        self.visited.truncate(self.cursor + 1);
        self.visited.push(page);
        self.cursor += 1;
        true
    }

    pub(super) fn back(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        true
    }

    pub(super) fn forward(&mut self) -> bool {
        if self.cursor + 1 >= self.visited.len() {
            return false;
        }
        self.cursor += 1;
        true
    }
}

impl ToksApp {
    pub(crate) fn page(&self) -> Page {
        self.navigation.current()
    }

    pub(crate) fn navigate_to(&mut self, page: Page) -> bool {
        self.navigation.visit(page)
    }

    pub(crate) fn navigate_back(&mut self) -> bool {
        self.navigation.back()
    }

    pub(crate) fn navigate_forward(&mut self) -> bool {
        self.navigation.forward()
    }
}
