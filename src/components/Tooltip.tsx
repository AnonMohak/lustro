import { useEffect, useLayoutEffect, useRef, useState } from "react";

const DELAY = 500;
const GAP = 6;
const EDGE = 8;

interface Tip {
  text: string;
  /** The hovered or focused element. */
  rect: DOMRect;
  /** The pointer, or the left edge of a focused element. */
  x: number;
}

/**
 * One app-styled tooltip for every element with a `title`. While an element shows its tip, its
 * `title` moves to `data-tip`, so Windows does not draw its own tooltip as well.
 */
export function Tooltip() {
  const [tip, setTip] = useState<Tip | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useEffect(() => {
    let el: HTMLElement | null = null;
    let timer: number | undefined;
    let mouseX = 0;

    const hide = () => {
      clearTimeout(timer);
      setTip(null);
    };
    // Give the element its title back.
    const release = () => {
      hide();
      if (el) {
        if (!el.hasAttribute("title") && el.dataset.tip) el.setAttribute("title", el.dataset.tip);
        delete el.dataset.tip;
        el.removeAttribute("aria-describedby");
      }
      el = null;
    };
    const take = (target: EventTarget | null, fromMouse: boolean) => {
      const next = target instanceof Element ? target.closest<HTMLElement>("[title], [data-tip]") : null;
      if (next === el) return;
      release();
      const text = next?.getAttribute("title");
      if (!next || !text?.trim()) return;
      el = next;
      el.dataset.tip = text;
      el.removeAttribute("title");
      timer = window.setTimeout(() => {
        if (!el?.isConnected) return;
        const rect = el.getBoundingClientRect();
        el.setAttribute("aria-describedby", "app-tip");
        setTip({ text, rect, x: fromMouse ? mouseX : rect.left });
      }, DELAY);
    };

    const onOver = (e: MouseEvent) => {
      mouseX = e.clientX;
      take(e.target, true);
    };
    const onMove = (e: MouseEvent) => {
      mouseX = e.clientX;
    };
    const onOut = (e: MouseEvent) => {
      if (!e.relatedTarget) release();
    };
    const onFocus = (e: FocusEvent) => {
      if (e.target instanceof Element && e.target.matches(":focus-visible")) take(e.target, false);
    };
    const onBlur = (e: FocusEvent) => {
      if (e.target === el) release();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hide();
    };
    const opts = { capture: true, passive: true };
    document.addEventListener("mouseover", onOver, opts);
    document.addEventListener("mousemove", onMove, opts);
    document.addEventListener("mouseout", onOut, opts);
    document.addEventListener("focusin", onFocus, opts);
    document.addEventListener("focusout", onBlur, opts);
    // A click, a scroll or Esc hides the tip until the pointer leaves the element.
    document.addEventListener("mousedown", hide, opts);
    document.addEventListener("wheel", hide, opts);
    document.addEventListener("scroll", hide, opts);
    document.addEventListener("keydown", onKey, opts);
    window.addEventListener("blur", release);
    return () => {
      release();
      document.removeEventListener("mouseover", onOver, opts);
      document.removeEventListener("mousemove", onMove, opts);
      document.removeEventListener("mouseout", onOut, opts);
      document.removeEventListener("focusin", onFocus, opts);
      document.removeEventListener("focusout", onBlur, opts);
      document.removeEventListener("mousedown", hide, opts);
      document.removeEventListener("wheel", hide, opts);
      document.removeEventListener("scroll", hide, opts);
      document.removeEventListener("keydown", onKey, opts);
      window.removeEventListener("blur", release);
    };
  }, []);

  // Below the element, or above it when there is no space below. Never past the window edge.
  useLayoutEffect(() => {
    if (!tip || !box.current) return setPos(null);
    const { width: w, height: h } = box.current.getBoundingClientRect();
    const below = tip.rect.bottom + GAP;
    const top = below + h > window.innerHeight - EDGE ? Math.max(EDGE, tip.rect.top - GAP - h) : below;
    const left = Math.max(EDGE, Math.min(tip.x, window.innerWidth - w - EDGE));
    setPos({ left, top });
  }, [tip]);

  if (!tip) return null;
  return (
    <div
      ref={box}
      id="app-tip"
      className="tip"
      role="tooltip"
      style={pos ? { left: pos.left, top: pos.top } : { left: 0, top: 0, visibility: "hidden" }}
    >
      {tip.text}
    </div>
  );
}
