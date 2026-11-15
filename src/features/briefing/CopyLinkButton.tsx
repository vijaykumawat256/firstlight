import clsx from "clsx";
import { Copy } from "lucide-react";
import { useToasts } from "@/design";
import { copyLink } from "./copyLink";

/** A small copy icon that sits right after a link, so it is clear which link it copies. */
export function CopyLinkButton({ url, label = "Copy link", className }: { url: string; label?: string; className?: string }) {
  const push = useToasts((s) => s.push);
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={(e) => {
        e.stopPropagation();
        void copyLink(url, push);
      }}
      className={clsx(
        "inline-flex size-6 shrink-0 items-center justify-center rounded-full text-fg-2 transition-[background-color,color] duration-150 ease-standard hover:bg-fill-strong hover:text-fg active:bg-fill-strong",
        className,
      )}
    >
      <Copy className="size-3.5" aria-hidden />
    </button>
  );
}
