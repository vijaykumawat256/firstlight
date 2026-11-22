import clsx from "clsx";
import type { CSSProperties, HTMLAttributes, ReactNode } from "react";

/* ------------------------------------------------------------------ Card */

export function Card({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div className={clsx("rounded-card bg-surface p-4 shadow-card ring-1 ring-separator", className)} {...rest} />;
}

/* ----------------------------------------------------- ListGroup / Row */

/** A rounded group of rows divided by hairline separators. */
export function ListGroup({ className, ...rest }: HTMLAttributes<HTMLUListElement>) {
  return (
    <ul
      className={clsx("divide-y divide-separator overflow-hidden rounded-card bg-surface shadow-card ring-1 ring-separator", className)}
      {...rest}
    />
  );
}

export interface ListRowProps {
  leading?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  trailing?: ReactNode;
  dimmed?: boolean;
  onClick?: () => void;
  className?: string;
}

export function ListRow({ leading, title, subtitle, trailing, dimmed, onClick, className }: ListRowProps) {
  const body = (
    <>
      {leading}
      <div className="min-w-0 flex-1 text-left">
        <div className="truncate text-headline font-medium text-fg">{title}</div>
        {subtitle && <div className="truncate text-footnote text-fg-2">{subtitle}</div>}
      </div>
      {trailing && <div className="shrink-0 text-footnote text-fg-2">{trailing}</div>}
    </>
  );
  const cls = clsx(
    "flex min-h-12 w-full items-center gap-3 px-4 py-2 transition-[background-color,opacity] duration-150 ease-standard",
    dimmed && "opacity-50",
    onClick && "hover:bg-fill active:bg-fill-strong",
    className,
  );
  return (
    <li>
      {onClick ? (
        <button type="button" className={cls} onClick={onClick}>
          {body}
        </button>
      ) : (
        <div className={cls}>{body}</div>
      )}
    </li>
  );
}

/* ----------------------------------------------------------------- Badge */

type BadgeTone = "neutral" | "accent" | "danger" | "warning";

const badgeTones: Record<BadgeTone, string> = {
  neutral: "bg-fill text-fg-2",
  accent: "bg-accent-soft text-accent",
  danger: "bg-danger-soft text-danger",
  warning: "bg-warning-soft text-warning",
};

export function Badge({ tone = "neutral", children }: { tone?: BadgeTone; children: ReactNode }) {
  return (
    <span
      className={clsx(
        "inline-flex h-5 items-center rounded-full px-2 text-caption font-semibold",
        badgeTones[tone],
      )}
    >
      {children}
    </span>
  );
}

/* ------------------------------------------------------------ EmptyState */

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-2 px-6 py-10 text-center animate-fade-in">
      {icon && <div className="mb-1 text-fg-3 [&>svg]:size-10 [&>svg]:stroke-[1.25]">{icon}</div>}
      <h2 className="text-headline font-semibold text-fg">{title}</h2>
      {description && <p className="max-w-[28ch] text-body text-fg-2">{description}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/* -------------------------------------------------------------- Skeleton */

export function Skeleton({ className, style }: { className?: string; style?: CSSProperties }) {
  return <div aria-hidden style={style} className={clsx("animate-pulse rounded-control bg-fill-strong", className)} />;
}

/* --------------------------------------------------------- SectionHeader */

export function SectionHeader({ title, trailing }: { title: string; trailing?: ReactNode }) {
  return (
    <div className="mb-2 flex items-baseline justify-between gap-3 px-1">
      <h2 className="text-headline font-semibold tracking-tight text-fg">{title}</h2>
      {trailing && <div className="truncate text-footnote text-fg-2">{trailing}</div>}
    </div>
  );
}
