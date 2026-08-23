import type { ReactNode } from "react";
import type { Box } from "../lib/pack";

/** The tile chrome from the design handoff: a grab bar with a grip and a close button, the body,
 *  and a resize handle in the corner.
 *
 *  Dragging is deliberately restricted to the bar. A panel full of tasks you might want to select
 *  text in is not a drag surface, and the prototype's `grab` flag exists for exactly that reason. */
export function Tile({
  box, title, sub, dragging, onGrab, onDragStart, onDragOver, onDragEnd, onClose, onResizeStart, children,
}: {
  box: Box;
  title: string;
  sub?: ReactNode;
  dragging: boolean;
  onGrab: (grabbed: boolean) => void;
  onDragStart: (e: React.DragEvent) => void;
  onDragOver: (e: React.DragEvent) => void;
  onDragEnd: () => void;
  onClose: () => void;
  onResizeStart: (e: React.PointerEvent) => void;
  children: ReactNode;
}) {
  return (
    <section
      className={dragging ? "tile dragging" : "tile"}
      style={{ left: box.left, top: box.top, width: box.width, height: box.height }}
      draggable
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      aria-label={title}
    >
      <header
        className="tilebar"
        onMouseDown={() => onGrab(true)}
        onMouseUp={() => onGrab(false)}
      >
        <span className="tilegrip" aria-hidden="true">
          <svg width="10" height="12" viewBox="0 0 10 12" fill="currentColor">
            <circle cx="2" cy="2" r="1" /><circle cx="8" cy="2" r="1" />
            <circle cx="2" cy="6" r="1" /><circle cx="8" cy="6" r="1" />
            <circle cx="2" cy="10" r="1" /><circle cx="8" cy="10" r="1" />
          </svg>
        </span>
        <span className="tiletitle">{title}</span>
        {sub ? <span className="tilesub">{sub}</span> : null}
        <button
          className="tileclose"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={onClose}
          aria-label={`Close ${title}`}
          title="Close"
        >
          ✕
        </button>
      </header>
      <div className="tilebody">{children}</div>
      <span
        className="tileresize"
        onPointerDown={onResizeStart}
        title="Resize"
        aria-hidden="true"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M11 4 4 11M11 8l-3 3" />
        </svg>
      </span>
    </section>
  );
}
