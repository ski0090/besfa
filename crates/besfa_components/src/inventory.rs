//! Items kept in slots, as a bag or a chest keeps them: stacks of one item
//! each, up to a limit the game gives for that item. Things go in past the
//! slots as well; what holding more than the slots means is the game's.
//! [`Equipment`] keeps what is worn, one stack to a named slot.

use bevy::prelude::*;

/// Stacks of items, `slots` of them before it is over.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::inventory"]
#[reflect(Component, Default)]
pub struct Inventory {
    pub slots: usize,
    pub stacks: Vec<ItemStack>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: 16,
            stacks: Vec::new(),
        }
    }
}

/// Some of one item. A worn one, a tool used for a while, and one with
/// something fitted to it stay in stacks of their own.
#[derive(Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::inventory"]
#[reflect(Default)]
pub struct ItemStack {
    pub item: String,
    pub count: u32,
    /// How much it has been used, for things that wear out.
    pub wear: u32,
    /// What is fitted to it, as runes or gems in a weapon's sockets, by name.
    #[reflect(default)]
    pub attached: Vec<String>,
}

impl ItemStack {
    pub fn new(item: &str, count: u32) -> Self {
        Self {
            item: item.to_owned(),
            count,
            wear: 0,
            attached: Vec::new(),
        }
    }
}

impl Inventory {
    pub fn new(slots: usize) -> Self {
        Self {
            slots,
            stacks: Vec::new(),
        }
    }

    /// Puts `count` of `item` in, `limit` to a stack at most: tops up its
    /// unworn stacks first, then takes new ones, past the slots once they
    /// are all taken.
    pub fn add(&mut self, item: &str, count: u32, limit: u32) {
        self.put(ItemStack::new(item, count), limit);
    }

    /// Puts a stack in as [`Inventory::add`] does; a worn one, or one with
    /// something fitted, keeps a stack of its own.
    pub fn put(&mut self, mut stack: ItemStack, limit: u32) {
        let limit = limit.max(1);
        let plain = |stack: &ItemStack| stack.wear == 0 && stack.attached.is_empty();
        if plain(&stack) {
            for slot in self
                .stacks
                .iter_mut()
                .filter(|slot| slot.item == stack.item && plain(slot))
            {
                let taken = stack.count.min(limit.saturating_sub(slot.count));
                slot.count += taken;
                stack.count -= taken;
            }
        }
        while stack.count > 0 {
            let taken = stack.count.min(limit);
            self.stacks.push(ItemStack {
                count: taken,
                ..stack.clone()
            });
            stack.count -= taken;
        }
    }

    /// How many of `item` it holds.
    pub fn count(&self, item: &str) -> u32 {
        self.stacks
            .iter()
            .filter(|slot| slot.item == item)
            .map(|slot| slot.count)
            .sum()
    }

    /// Takes `count` of `item` out, from its last stacks first, freeing the
    /// slots it empties; takes nothing and says false if it holds fewer.
    pub fn take(&mut self, item: &str, mut count: u32) -> bool {
        if self.count(item) < count {
            return false;
        }
        for slot in self.stacks.iter_mut().rev().filter(|slot| slot.item == item) {
            let taken = count.min(slot.count);
            slot.count -= taken;
            count -= taken;
        }
        self.stacks.retain(|slot| slot.count > 0);
        true
    }

    /// Whether it holds more stacks than it has slots.
    pub fn over(&self) -> bool {
        self.stacks.len() > self.slots
    }

    /// Whether `stack` would go in without going past the slots.
    pub fn fits(&self, stack: &ItemStack, limit: u32) -> bool {
        let mut after = self.clone();
        after.put(stack.clone(), limit);
        after.stacks.len() <= self.slots
    }
}

/// What is worn, one stack or none to each named slot. Which slots there
/// are and what each takes is the game's.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::inventory"]
#[reflect(Component, Default)]
pub struct Equipment {
    pub slots: Vec<EquipSlot>,
}

#[derive(Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::inventory"]
#[reflect(Default)]
pub struct EquipSlot {
    pub name: String,
    pub worn: Option<ItemStack>,
}

impl Equipment {
    /// Empty slots of these names.
    pub fn new(slots: &[&str]) -> Self {
        Self {
            slots: slots
                .iter()
                .map(|name| EquipSlot {
                    name: (*name).to_owned(),
                    worn: None,
                })
                .collect(),
        }
    }

    /// What `slot` holds.
    pub fn get(&self, slot: &str) -> Option<&ItemStack> {
        self.slots.iter().find(|held| held.name == slot)?.worn.as_ref()
    }

    pub fn get_mut(&mut self, slot: &str) -> Option<&mut ItemStack> {
        self.slots.iter_mut().find(|held| held.name == slot)?.worn.as_mut()
    }

    /// Puts `stack` in `slot`, giving back what it held; a slot it does not
    /// have gives `stack` itself back.
    pub fn wear(&mut self, slot: &str, stack: ItemStack) -> Option<ItemStack> {
        match self.slots.iter_mut().find(|held| held.name == slot) {
            Some(held) => held.worn.replace(stack),
            None => Some(stack),
        }
    }

    /// Empties `slot`, giving back what it held.
    pub fn take_off(&mut self, slot: &str) -> Option<ItemStack> {
        self.slots.iter_mut().find(|held| held.name == slot)?.worn.take()
    }

    /// Each worn stack.
    pub fn worn(&self) -> impl Iterator<Item = &ItemStack> {
        self.slots.iter().filter_map(|held| held.worn.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wears_one_stack_to_a_slot_and_gives_back_the_one_before() {
        let mut worn = Equipment::new(&["body", "head"]);
        assert_eq!(worn.wear("body", ItemStack::new("cloth", 1)), None);
        let old = worn.wear("body", ItemStack::new("leather", 1));
        assert_eq!(old.map(|stack| stack.item), Some("cloth".into()));
        assert_eq!(worn.get("body").map(|stack| stack.item.as_str()), Some("leather"));
        // No such slot: the stack comes back.
        let back = worn.wear("ring", ItemStack::new("ring", 1));
        assert_eq!(back.map(|stack| stack.item), Some("ring".into()));
        assert_eq!(worn.worn().count(), 1);
        assert_eq!(worn.take_off("body").map(|stack| stack.item), Some("leather".into()));
        assert_eq!(worn.get("body"), None);
    }

    #[test]
    fn stacks_up_to_the_limit_and_goes_past_its_slots() {
        let mut bag = Inventory::new(3);
        bag.add("log", 70, 50);
        bag.add("stone", 10, 50);
        assert!(!bag.over());
        // Tops up the second log stack, then goes past the slots.
        bag.add("log", 40, 50);
        let counts: Vec<_> = bag
            .stacks
            .iter()
            .map(|stack| (stack.item.as_str(), stack.count))
            .collect();
        assert_eq!(counts, [("log", 50), ("log", 50), ("stone", 10), ("log", 10)]);
        assert!(bag.over());
        assert!(!bag.fits(&ItemStack::new("stone", 1), 50));
        // Back within its slots once the rest is used up.
        assert!(bag.take("log", 10));
        assert!(!bag.over());
        assert!(!bag.take("log", 101), "takes nothing when short");
        assert_eq!(bag.count("log"), 100);
    }

    #[test]
    fn worn_things_keep_stacks_of_their_own() {
        let mut bag = Inventory::new(4);
        bag.add("axe", 2, 1);
        bag.stacks[0].wear = 3;
        bag.put(
            ItemStack {
                wear: 5,
                ..ItemStack::new("axe", 1)
            },
            1,
        );
        assert_eq!(bag.stacks.len(), 3);
        assert_eq!(bag.count("axe"), 3);
        // Limits of 1 never merge, worn or not.
        bag.add("axe", 1, 1);
        assert_eq!(bag.stacks.len(), 4);
        // Nor does one with something fitted.
        let mut gems = Inventory::new(4);
        gems.add("sword", 1, 5);
        gems.put(
            ItemStack {
                attached: vec!["ruby".into()],
                ..ItemStack::new("sword", 1)
            },
            5,
        );
        assert_eq!(gems.stacks.len(), 2);
    }
}
