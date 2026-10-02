import { SvelteSet } from "svelte/reactivity";

class Selection {
  active = $state(false);
  keys = new SvelteSet();

  get count() {
    return this.keys.size;
  }

  has(key) {
    return this.keys.has(key);
  }

  enter(key = null) {
    this.active = true;
    if (key) this.keys.add(key);
  }

  add(key) {
    this.keys.add(key);
  }

  toggle(key) {
    if (!this.keys.delete(key)) this.keys.add(key);
  }

  selectAll(keys) {
    this.active = true;
    keys.forEach((key) => this.keys.add(key));
  }

  clear() {
    this.keys.clear();
  }

  exit() {
    this.keys.clear();
    this.active = false;
  }

  masterState(visibleKeys) {
    if (!visibleKeys.length || !this.keys.size) return "none";
    const selected = visibleKeys.filter((key) => this.keys.has(key)).length;
    if (selected === 0) return "none";
    return selected === visibleKeys.length ? "all" : "some";
  }
}

export const selection = new Selection();
