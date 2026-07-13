import { defineComponent, h, type PropType, type VNode } from "vue";
import type { KeyTreeNode } from "../lib/api";

const KeyTreeItem: ReturnType<typeof defineComponent> = defineComponent({
  name: "KeyTreeItem",
  props: {
    node: { type: Object as PropType<KeyTreeNode>, required: true },
    depth: { type: Number, required: true },
    expanded: { type: Object as PropType<Set<string>>, required: true },
  },
  emits: ["toggle", "select"],
  setup(props, { emit }) {
    const path = () => `${props.depth}:${props.node.name}:${props.node.fullKey ?? ""}`;
    return (): VNode => {
      const kids = props.node.children;
      const isOpen = props.expanded.has(path());
      const row = h(
        "div",
        {
          class: "flex items-center gap-1 px-2 py-0.5 rounded cursor-pointer text-zinc-400 hover:bg-zinc-900",
          style: { paddingLeft: `${8 + props.depth * 12}px` },
          onClick: () => {
            if (kids.length) emit("toggle", path());
            if (props.node.fullKey) emit("select", props.node.fullKey);
          },
        },
        [
          kids.length
            ? h("span", { class: "text-zinc-600 w-3" }, isOpen ? "▾" : "▸")
            : h("span", { class: "w-3" }),
          h("span", { class: props.node.fullKey ? "" : "text-zinc-500" }, props.node.name),
          kids.length ? h("span", { class: "text-zinc-600 text-[10px]" }, String(props.node.count)) : null,
        ],
      );
      const children = isOpen
        ? kids.map((c) =>
            h(KeyTreeItem, {
              node: c,
              depth: props.depth + 1,
              expanded: props.expanded,
              onToggle: (p: string) => emit("toggle", p),
              onSelect: (k: string) => emit("select", k),
            }),
          )
        : [];
      return h("div", null, [row, ...children]);
    };
  },
});

export default KeyTreeItem;
