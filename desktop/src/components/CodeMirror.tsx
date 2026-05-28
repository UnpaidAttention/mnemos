import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { markdown } from "@codemirror/lang-markdown";

export function CodeMirrorView({ value }: { value: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    const view = new EditorView({
      state: EditorState.create({
        doc: value,
        extensions: [
          markdown(),
          EditorView.editable.of(false),
          EditorView.lineWrapping,
        ],
      }),
      parent: ref.current,
    });
    return () => view.destroy();
  }, [value]);
  return (
    <div
      ref={ref}
      className="mono text-sm border border-border rounded-md max-h-80 overflow-auto"
    />
  );
}
