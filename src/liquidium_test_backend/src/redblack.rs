use std::cmp::Ordering;
use std::mem;

#[derive(Clone, Debug)]
pub enum Color {
    Red,
    Black,
}

#[derive(Clone, Debug)]
pub struct Node<T: Ord> {
    value: T,
    color: Color,
    left: Option<Box<Node<T>>>,
    right: Option<Box<Node<T>>>,
}

#[derive(Clone, Debug)]
pub struct RedBlackTree<T: Ord> {
    root: Option<Box<Node<T>>>,
    size: usize,
}

impl<T: Ord> Node<T> {
    fn new(value: T) -> Self {
        Node {
            value,
            color: Color::Red,
            left: None,
            right: None,
        }
    }
}

impl<T: Ord + Clone> RedBlackTree<T> {
    pub fn new() -> Self {
        RedBlackTree {
            root: None,
            size: 0,
        }
    }

    pub fn insert(&mut self, value: T) {
        self.size += 1;
        let root = mem::replace(&mut self.root, None);
        self.root = Some(self.insert_recursive(root, value));
        if let Some(ref mut root) = self.root {
            root.color = Color::Black;
        }
    }

    fn insert_recursive(&mut self, node: Option<Box<Node<T>>>, value: T) -> Box<Node<T>> {
        match node {
            None => Box::new(Node::new(value)),
            Some(mut node) => {
                match value.cmp(&node.value) {
                    Ordering::Less => {
                        node.left = Some(self.insert_recursive(node.left.take(), value));
                    }
                    Ordering::Greater => {
                        node.right = Some(self.insert_recursive(node.right.take(), value));
                    }
                    Ordering::Equal => return node,
                }
                self.balance(node)
            }
        }
    }

    fn balance(&mut self, mut node: Box<Node<T>>) -> Box<Node<T>> {
        if self.is_red(&node.right) && !self.is_red(&node.left) {
            node = self.rotate_left(node);
        }
        if self.is_red(&node.left) && self.is_red(&node.left.as_ref().unwrap().left) {
            node = self.rotate_right(node);
        }
        if self.is_red(&node.left) && self.is_red(&node.right) {
            self.flip_colors(&mut node);
        }
        node
    }

    fn is_red(&self, node: &Option<Box<Node<T>>>) -> bool {
        match node {
            Some(n) => matches!(n.color, Color::Red),
            None => false,
        }
    }

    fn rotate_left(&mut self, mut node: Box<Node<T>>) -> Box<Node<T>> {
        let mut right = node.right.take().unwrap();
        let color = node.color.clone();
        node.right = right.left.take();
        right.left = Some(node);
        right.color = color;
        right.left.as_mut().unwrap().color = Color::Red;
        right
    }

    fn rotate_right(&mut self, mut node: Box<Node<T>>) -> Box<Node<T>> {
        let mut left = node.left.take().unwrap();
        node.left = left.right.take();
        let color = node.color.clone();
        left.right = Some(node);
        left.color = color;
        left.right.as_mut().unwrap().color = Color::Red;
        left
    }

    fn flip_colors(&mut self, node: &mut Box<Node<T>>) {
        node.color = Color::Red;
        if let Some(ref mut left) = node.left {
            left.color = Color::Black;
        }
        if let Some(ref mut right) = node.right {
            right.color = Color::Black;
        }
    }

    pub fn remove_less_than_or_equal(&mut self, value: &T) -> Vec<T> {
        let mut removed = Vec::new();
        let x = self.root.take();
        self.root = self.remove_less_than_or_equal_recursive(x, value, &mut removed);
        self.size -= removed.len();
        if let Some(ref mut root) = self.root {
            root.color = Color::Black;
        }
        removed
    }

    fn remove_less_than_or_equal_recursive(
        &mut self,
        node: Option<Box<Node<T>>>,
        value: &T,
        removed: &mut Vec<T>
    ) -> Option<Box<Node<T>>> {
        match node {
            None => None,
            Some(mut boxed_node) => {
                match boxed_node.value.cmp(value) {
                    Ordering::Less | Ordering::Equal => {
                        // Remove this node and its left subtree
                        removed.extend(self.collect_all_values(boxed_node.left));
                        removed.push(boxed_node.value.clone());
                        // Process the right subtree
                        self.remove_less_than_or_equal_recursive(boxed_node.right, value, removed)
                    },
                    Ordering::Greater => {
                        // Process left subtree
                        boxed_node.left = self.remove_less_than_or_equal_recursive(boxed_node.left, value, removed);
                        Some(boxed_node)
                    }
                }
            }
        }
    }

    fn collect_all_values(&self, node: Option<Box<Node<T>>>) -> Vec<T> {
        let mut values = Vec::new();
        if let Some(boxed_node) = node {
            values.extend(self.collect_all_values(boxed_node.left));
            values.push(boxed_node.value.clone());
            values.extend(self.collect_all_values(boxed_node.right));
        }
        values
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn contains(&self, value: &T) -> bool {
        let mut current = &self.root;
        while let Some(node) = current {
            match value.cmp(&node.value) {
                Ordering::Less => current = &node.left,
                Ordering::Greater => current = &node.right,
                Ordering::Equal => return true,
            }
        }
        false
    }

    pub fn inorder_traversal(&self) -> Vec<&T> {
        let mut result = Vec::with_capacity(self.size);
        self.inorder_recursive(&self.root, &mut result);
        result
    }

    fn inorder_recursive<'a>(&'a self, node: &'a Option<Box<Node<T>>>, result: &mut Vec<&'a T>) {
        if let Some(node) = node {
            self.inorder_recursive(&node.left, result);
            result.push(&node.value);
            self.inorder_recursive(&node.right, result);
        }
    }
}

// Example usage in an ICP canister
// use ic_cdk_macros::*;
// #[update]
// fn insert(value: i32) {
//     SORTED_DATA.with(|data| data.borrow_mut().insert(value));
// }

// #[query]
// fn get_sorted() -> Vec<i32> {
//     SORTED_DATA.with(|data| data.borrow().inorder_traversal().iter().map(|&&x| x).collect())
// }

// #[query]
// fn contains(value: i32) -> bool {
//     SORTED_DATA.with(|data| data.borrow().contains(&value))
// }

// #[query]
// fn size() -> usize {
//     SORTED_DATA.with(|data| data.borrow().len())
// }