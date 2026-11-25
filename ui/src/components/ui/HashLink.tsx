// Solite's own file: a link to a hash address (`#/zone/<id>`, `#/equipments/<id>`...). Solite has no
// router (decision D19): a plain click sets `location.hash`, as the links of the Equipments and
// Devices pages do; a click with a modifier key or a middle click keeps the browser's own way
// (a new tab).
import type { ComponentChildren } from "preact";

export function HashLink({
  href,
  className,
  title,
  children,
}: {
  href: string;
  className?: string;
  title?: string;
  children: ComponentChildren;
}) {
  return (
    <a
      href={href}
      title={title}
      className={className}
      onClick={(e) => {
        if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
        e.preventDefault();
        window.location.hash = href;
      }}
    >
      {children}
    </a>
  );
}
