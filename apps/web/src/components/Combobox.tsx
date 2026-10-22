import { useEffect, useId, useRef, useState } from "react";

export type ComboboxOption = { value: string; label: string; group?: string };

export function Combobox({
  label,
  value,
  options,
  onChange,
  className,
}: {
  label: string;
  value: string;
  options: ComboboxOption[];
  onChange: (value: string) => void;
  className?: string;
}) {
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const selected = options.find((option) => option.value === value);
  const shown = options.filter((option) =>
    `${option.label} ${option.group ?? ""}`.toLowerCase().includes(query.trim().toLowerCase()),
  );

  useEffect(() => {
    if (!open) return;
    function close(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);

  function choose(next: string) {
    onChange(next);
    setQuery("");
    setOpen(false);
  }

  function onKey(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setOpen(true);
      setActive((index) => Math.min(index + 1, Math.max(0, shown.length - 1)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((index) => Math.max(index - 1, 0));
    } else if (event.key === "Enter" && open) {
      event.preventDefault();
      const option = shown[active];
      if (option) choose(option.value);
    } else if (event.key === "Escape") {
      setOpen(false);
      setQuery("");
    }
  }

  const groups: Array<{ name: string; options: ComboboxOption[] }> = [];
  for (const option of shown) {
    const name = option.group ?? "";
    const last = groups[groups.length - 1];
    if (last && last.name === name) last.options.push(option);
    else groups.push({ name, options: [option] });
  }

  let index = -1;
  return (
    <div className={`combo ${className ?? ""}`} ref={rootRef}>
      <span className="combo-label" id={`${listId}-label`}>{label}</span>
      <input
        role="combobox"
        aria-labelledby={`${listId}-label`}
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        value={open ? query : selected?.label ?? ""}
        placeholder={selected?.label ?? "Type to search"}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
          setActive(0);
        }}
        onFocus={() => {
          setQuery("");
          setOpen(true);
        }}
        onKeyDown={onKey}
      />
      {open && (
        <ul className="combo-list" id={listId} role="listbox">
          {groups.map((group) => (
            <li key={group.name || "all"}>
              {group.name && <p className="combo-group">{group.name}</p>}
              <ul>
                {group.options.map((option) => {
                  index += 1;
                  const itemIndex = index;
                  return (
                    <li key={option.value}>
                      <button
                        type="button"
                        role="option"
                        aria-selected={option.value === value}
                        data-active={itemIndex === active ? "true" : undefined}
                        onMouseEnter={() => setActive(itemIndex)}
                        onClick={() => choose(option.value)}
                      >
                        {option.label}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </li>
          ))}
          {shown.length === 0 && <li className="combo-empty">No match</li>}
        </ul>
      )}
    </div>
  );
}
