import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon";

export interface SelectOption<T> {
  value: T;
  label: string;
}

const GAP = 4;
const MAX_H = 320;

/** A drop-down in the app style. It replaces <select>, whose open list Windows draws. */
export function Select<T extends string | number>({
  value,
  options,
  onChange,
  className = "text",
  id,
  ariaLabel,
  disabled,
}: {
  value: T;
  options: SelectOption<T>[];
  onChange: (v: T) => void;
  /** The look of the closed box: "text" or "fchip …". */
  className?: string;
  id?: string;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  const listId = useId();
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [pos, setPos] = useState<{ left: number; top: number; minWidth: number; maxHeight: number } | null>(null);
  const cur = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );

  const show = () => {
    if (disabled) return;
    setActive(cur);
    setOpen(true);
  };
  const pick = (i: number) => {
    setOpen(false);
    btn.current?.focus();
    if (options[i] && options[i].value !== value) onChange(options[i].value);
  };

  // Below the box, or above it when there is no space below.
  useLayoutEffect(() => {
    if (!open || !btn.current || !list.current) return;
    const r = btn.current.getBoundingClientRect();
    const h = Math.min(list.current.scrollHeight, MAX_H);
    const below = window.innerHeight - r.bottom - GAP - 8;
    const up = below < h && r.top > below;
    const maxHeight = Math.min(MAX_H, up ? r.top - GAP - 8 : below);
    const left = Math.min(r.left, window.innerWidth - Math.max(r.width, list.current.offsetWidth) - 8);
    setPos({ left: Math.max(8, left), top: up ? r.top - GAP - Math.min(h, maxHeight) : r.bottom + GAP, minWidth: r.width, maxHeight });
  }, [open]);

  useEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const close = (e: Event) => {
      if (e.target instanceof Node && (list.current?.contains(e.target) || btn.current?.contains(e.target))) return;
      setOpen(false);
    };
    const shut = () => setOpen(false);
    window.addEventListener("mousedown", close, true);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", shut);
    window.addEventListener("blur", shut);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", shut);
      window.removeEventListener("blur", shut);
    };
  }, [open]);

  // Keep the active option in view.
  useEffect(() => {
    if (open) list.current?.children[active]?.scrollIntoView({ block: "nearest" });
  }, [open, active, pos]);

  const onKey = (e: KeyboardEvent) => {
    const last = options.length - 1;
    const move = (i: number) => {
      e.preventDefault();
      if (open) setActive(Math.max(0, Math.min(last, i)));
      else show();
    };
    switch (e.key) {
      case "ArrowDown":
        return move(active + 1);
      case "ArrowUp":
        return move(active - 1);
      case "Home":
        return open && move(0);
      case "End":
        return open && move(last);
      case "Enter":
      case " ":
        e.preventDefault();
        return open ? pick(active) : show();
      case "Escape":
        if (open) {
          e.preventDefault();
          e.stopPropagation();
          setOpen(false);
        }
        return;
      case "Tab":
        return setOpen(false);
      default:
        // Type a letter to jump to the next option that starts with it.
        if (open && e.key.length === 1) {
          const k = e.key.toLowerCase();
          const n = options.length;
          for (let s = 1; s <= n; s++) {
            const i = (active + s) % n;
            if (options[i].label.toLowerCase().startsWith(k)) return setActive(i);
          }
        }
    }
  };

  return (
    <>
      <button
        ref={btn}
        type="button"
        id={id}
        className={`${className} select ${open ? "open" : ""}`}
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open ? `${listId}-${active}` : undefined}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : show())}
        onKeyDown={onKey}
      >
        <span className="sv">{options[cur]?.label}</span>
        <Icon name="chev" />
      </button>
      {open &&
        createPortal(
          <div
            ref={list}
            id={listId}
            className="menu"
            role="listbox"
            style={pos ? { left: pos.left, top: pos.top, minWidth: pos.minWidth, maxHeight: pos.maxHeight } : { visibility: "hidden" }}
          >
            {options.map((o, i) => (
              <div
                key={String(o.value)}
                id={`${listId}-${i}`}
                role="option"
                aria-selected={i === cur}
                className={`mi ${i === active ? "act" : ""} ${i === cur ? "on" : ""}`}
                onMouseMove={() => i !== active && setActive(i)}
                onClick={() => pick(i)}
              >
                <Icon name="check" />
                <span>{o.label}</span>
              </div>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}
