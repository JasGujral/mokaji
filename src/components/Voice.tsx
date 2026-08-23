import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { Action, Applied, Preview } from "../lib/types";

/** Anything the overlay can do to the rest of the app. Passing these in rather than reaching for
 *  a global store keeps the overlay a leaf: it decides *what* was said, App decides *what
 *  happens*, and the two can be tested apart. */
export interface VoiceHandlers {
  panel: (name: string, on: boolean) => void;
  ui: (name: string) => void;
  wrote: () => void;
}

/** Speech recognition, if the webview has it.
 *
 *  **V-2 is the reason this is a fallback rather than the plan.** WKWebView ships no
 *  `SpeechRecognition`, so on the packaged macOS app this is `undefined` and the typed field is
 *  the whole interface until the local whisper.cpp pipeline lands. Saying so in the overlay is
 *  better than a microphone button that silently does nothing. */
type Recognizer = { start(): void; stop(): void; abort(): void } & Record<string, unknown>;
function recognizerCtor(): (new () => Recognizer) | undefined {
  const w = window as unknown as Record<string, unknown>;
  return (w.SpeechRecognition ?? w.webkitSpeechRecognition) as (new () => Recognizer) | undefined;
}

const EQ_BARS = 9;

export function Voice({ open, onClose, handlers }: {
  open: boolean;
  onClose: () => void;
  handlers: VoiceHandlers;
}) {
  const [text, setText] = useState("");
  const [action, setAction] = useState<Action | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [reply, setReply] = useState<string | null>(null);
  const [listening, setListening] = useState(false);
  const [undo, setUndo] = useState<{ id: string; left: number } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const rec = useRef<Recognizer | null>(null);

  // Focus on open; clear on close. An overlay that keeps the last utterance around is one you have
  // to clear before you can use it, which is a tax on the fastest path in the app.
  useEffect(() => {
    if (open) {
      input.current?.focus();
    } else {
      setText(""); setAction(null); setPreview(null); setReply(null);
      rec.current?.abort();
      setListening(false);
    }
  }, [open]);

  // CON-4: say what it will do *before* it does it. Re-parsing on every keystroke is what makes
  // that sentence feel like a readout rather than a confirmation dialog.
  useEffect(() => {
    if (!open) return;
    const t = setTimeout(() => {
      const q = text.trim();
      if (!q) { setAction(null); setPreview(null); return; }
      void api.act(q).then(setAction).catch(() => setAction(null));
      void api.preview(q).then(setPreview).catch(() => setPreview(null));
    }, 90);
    return () => clearTimeout(t);
  }, [text, open]);

  useEffect(() => {
    if (!undo) return;
    if (undo.left <= 0) { setUndo(null); return; }
    const t = setTimeout(() => setUndo({ ...undo, left: undo.left - 1 }), 1000);
    return () => clearTimeout(t);
  }, [undo]);

  const listen = useCallback(() => {
    const Ctor = recognizerCtor();
    if (!Ctor) {
      setReply("This window has no speech engine. Type for now — the local one lands with M-3.");
      return;
    }
    if (listening) { rec.current?.stop(); setListening(false); return; }
    const r = new Ctor();
    Object.assign(r, { continuous: false, interimResults: true, lang: navigator.language });
    (r as Record<string, unknown>).onresult = (e: {
      results: ArrayLike<ArrayLike<{ transcript: string }>>;
    }) => {
      let s = "";
      for (let i = 0; i < e.results.length; i += 1) s += e.results[i]?.[0]?.transcript ?? "";
      setText(s);
    };
    (r as Record<string, unknown>).onend = () => setListening(false);
    (r as Record<string, unknown>).onerror = () => { setListening(false); setReply("Could not hear that."); };
    rec.current = r;
    r.start();
    setListening(true);
  }, [listening]);

  const run = useCallback(async () => {
    const q = text.trim();
    if (!q || !action) return;
    try {
      switch (action.kind) {
        case "panel":
          handlers.panel(action.name, action.on);
          onClose();
          return;
        case "window":
          if (action.on) { await api.windowShow(); } else { await api.windowHide(); }
          onClose();
          return;
        case "open": {
          const hit = await api.openNote(action.query);
          setReply(`Opening ${hit}`);
          setTimeout(onClose, 700);
          return;
        }
        case "ui":
          handlers.ui(action.name);
          onClose();
          return;
        case "write": {
          const applied: Applied = await api.apply(q);
          handlers.wrote();
          setText("");
          setReply(`Written to ${applied.path}`);
          setUndo({ id: applied.undo_id, left: applied.undo_seconds });
          return;
        }
        case "brief": {
          // M-5's exit criterion is a briefing *spoken and not dismissed*, so the voice path
          // reads it rather than merely opening the panel. The panel opens too, because a
          // briefing you can also see is one you can check afterwards.
          const b = await api.briefing();
          handlers.panel("briefing", true);
          await api.speak(`${b.greeting} ${b.spoken}`);
          setReply(
            b.three_connector
              ? `Reading ${b.lines.length} lines from ${b.sources.length} senses.`
              : `Reading ${b.lines.length} lines — only ${b.sources.length} of 3 senses answered.`,
          );
          setTimeout(onClose, 900);
          return;
        }
        case "hush":
          await api.hush();
          onClose();
          return;
        case "unmatched":
          // CON-2: falling back to a model is a decision, and until M-4 wires the router in, the
          // honest answer is that nothing local matched. Guessing here is how a HUD earns distrust.
          setReply(`Nothing local matched "${action.text}". Try "help".`);
          return;
      }
    } catch (e) {
      setReply(String(e));
    }
  }, [text, action, handlers, onClose]);

  if (!open) return null;

  const willWrite = action?.kind === "write";
  const line =
    reply ??
    (action
      ? action.kind === "unmatched"
        ? "No local command matches that yet."
        : (preview?.describes ?? "")
      : "Say or type a command.");

  return (
    <div className="overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      {/* V-7: an unmissable indicator whenever the microphone is live. A HUD that listens without
          saying so is the anti-pattern this whole product argues against. */}
      {listening && <div className="mic-live" aria-hidden />}
      <div className="eq" aria-hidden>
        {Array.from({ length: EQ_BARS }, (_, i) => (
          <i key={i} className={listening ? "live" : ""} style={{ animationDelay: `${i * 90}ms` }} />
        ))}
      </div>

      <div className="heard">{text || <span className="ghost">Hey Kaji…</span>}</div>

      <input
        ref={input}
        value={text}
        placeholder="add a task to call the harbour office tomorrow"
        onChange={(e) => { setText(e.target.value); setReply(null); }}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); void run(); }
          if (e.key === "Escape") { e.preventDefault(); onClose(); }
        }}
      />

      <div className={`reply ${willWrite ? "write" : ""}`}>{line}</div>
      {willWrite && preview?.diff && <pre className="diff">{preview.diff}</pre>}

      {undo && (
        <button
          className="undo"
          onClick={() => { void api.undoWrite(undo.id).then(() => { setUndo(null); handlers.wrote(); setReply("Undone."); }); }}
        >
          Undo · {undo.left}s
        </button>
      )}

      <div className="hint">
        <button className={`mic ${listening ? "live" : ""}`} onClick={listen}>
          {listening ? "Listening" : "Speak"}
        </button>
        <span>⌥Space anywhere · Enter to run · Esc to dismiss</span>
      </div>
    </div>
  );
}
