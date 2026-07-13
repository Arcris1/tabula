import { defineComponent, h, ref, type VNode } from "vue";

/** Recursive collapsible JSON tree node. */
const JsonNode: ReturnType<typeof defineComponent> = defineComponent({
  name: "JsonNode",
  props: {
    label: { type: String, default: "" },
    value: { required: true },
    depth: { type: Number, default: 0 },
  },
  setup(props) {
    const open = ref(props.depth < 2);
    return (): VNode => {
      const v = props.value;
      const isObj = v && typeof v === "object";
      const pad = { paddingLeft: `${props.depth * 12}px` };
      const key = props.label
        ? h("span", { class: "text-sky-300" }, `${props.label}: `)
        : null;

      if (!isObj) {
        const cls =
          v === null ? "text-zinc-500 italic"
          : typeof v === "number" ? "text-amber-300"
          : typeof v === "boolean" ? "text-purple-300"
          : "text-emerald-300";
        return h("div", { class: "font-mono text-[12px] py-0.5", style: pad },
          [key, h("span", { class: cls }, v === null ? "null" : typeof v === "string" ? `"${v}"` : String(v))]);
      }

      const entries = Array.isArray(v)
        ? (v as unknown[]).map((x, i) => [String(i), x] as const)
        : Object.entries(v as Record<string, unknown>);
      const summary = Array.isArray(v) ? `[${entries.length}]` : `{${entries.length}}`;

      const header = h("div", {
        class: "font-mono text-[12px] py-0.5 cursor-pointer select-none hover:bg-zinc-900",
        style: pad,
        onClick: () => (open.value = !open.value),
      }, [
        h("span", { class: "text-zinc-600 w-3 inline-block" }, open.value ? "▾" : "▸"),
        key,
        h("span", { class: "text-zinc-500" }, summary),
      ]);

      if (!open.value) return header;
      const children = entries.map(([k, val]) =>
        h(JsonNode, { label: k, value: val, depth: props.depth + 1 }),
      );
      return h("div", null, [header, ...children]);
    };
  },
});

export default JsonNode;
