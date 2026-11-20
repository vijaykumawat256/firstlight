import * as RSwitch from "@radix-ui/react-switch";
import * as RSelect from "@radix-ui/react-select";
import * as RToggleGroup from "@radix-ui/react-toggle-group";
import { Check, ChevronDown } from "lucide-react";
import clsx from "clsx";
import { forwardRef, useId, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";

/* ---------------------------------------------------------------- Button */

type ButtonVariant = "primary" | "secondary" | "plain" | "danger";
type ButtonSize = "sm" | "md";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  leading?: ReactNode;
}

const buttonBase =
  "inline-flex items-center justify-center gap-1.5 font-semibold select-none whitespace-nowrap " +
  "transition-[background-color,opacity,transform] duration-150 ease-standard active:scale-[0.98] " +
  "disabled:opacity-40 disabled:pointer-events-none";

const buttonVariants: Record<ButtonVariant, string> = {
  primary: "bg-accent-fill text-accent-fg hover:brightness-110 active:brightness-95",
  secondary: "bg-fill text-fg hover:bg-fill-strong",
  plain: "text-accent hover:bg-accent-soft",
  danger: "bg-danger-soft text-danger hover:brightness-95",
};

const buttonSizes: Record<ButtonSize, string> = {
  sm: "h-7 px-3 text-body rounded-control",
  md: "h-9 px-4 text-headline rounded-card",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", leading, className, children, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={clsx(buttonBase, buttonVariants[variant], buttonSizes[size], className)}
      {...rest}
    >
      {leading}
      {children}
    </button>
  );
});

/* ------------------------------------------------------------ IconButton */

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Required: icon-only buttons need an accessible name. */
  label: string;
  children: ReactNode;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, className, children, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      aria-label={label}
      title={label}
      className={clsx(
        "inline-flex size-8 items-center justify-center rounded-full text-fg-2",
        "transition-[background-color,color] duration-150 ease-standard hover:bg-fill hover:text-fg",
        "active:bg-fill-strong disabled:opacity-40",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
});

/* ---------------------------------------------------------------- Switch */

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}

export function Switch({ checked, onCheckedChange, label, disabled }: SwitchProps) {
  return (
    <RSwitch.Root
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      aria-label={label}
      className={clsx(
        "relative h-[26px] w-[44px] shrink-0 rounded-full transition-colors duration-200 ease-standard",
        "data-[state=checked]:bg-success data-[state=unchecked]:bg-fill-strong disabled:opacity-40",
      )}
    >
      <RSwitch.Thumb
        className={clsx(
          "block size-[22px] translate-x-[2px] rounded-full bg-white shadow-card",
          "transition-transform duration-200 ease-standard data-[state=checked]:translate-x-[20px]",
        )}
      />
    </RSwitch.Root>
  );
}

/* ---------------------------------------------------------------- Select */

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps {
  value: string;
  onValueChange: (value: string) => void;
  options: SelectOption[];
  label: string;
}

export function Select({ value, onValueChange, options, label }: SelectProps) {
  return (
    <RSelect.Root value={value} onValueChange={onValueChange}>
      <RSelect.Trigger
        aria-label={label}
        className={clsx(
          "inline-flex h-8 items-center gap-1.5 rounded-control bg-fill pl-3 pr-2 text-body text-fg",
          "transition-colors duration-150 ease-standard hover:bg-fill-strong",
        )}
      >
        <RSelect.Value />
        <RSelect.Icon>
          <ChevronDown className="size-3.5 text-fg-2" aria-hidden />
        </RSelect.Icon>
      </RSelect.Trigger>
      <RSelect.Portal>
        <RSelect.Content
          position="popper"
          sideOffset={6}
          className="z-50 min-w-[var(--radix-select-trigger-width)] animate-fade-in overflow-hidden rounded-card bg-surface-raised p-1 shadow-float"
        >
          <RSelect.Viewport>
            {options.map((o) => (
              <RSelect.Item
                key={o.value}
                value={o.value}
                className={clsx(
                  "relative flex h-7 cursor-default items-center rounded-control pl-7 pr-3 text-body text-fg outline-none",
                  "data-[highlighted]:bg-accent-fill data-[highlighted]:text-accent-fg",
                )}
              >
                <RSelect.ItemIndicator className="absolute left-2">
                  <Check className="size-3.5" aria-hidden />
                </RSelect.ItemIndicator>
                <RSelect.ItemText>{o.label}</RSelect.ItemText>
              </RSelect.Item>
            ))}
          </RSelect.Viewport>
        </RSelect.Content>
      </RSelect.Portal>
    </RSelect.Root>
  );
}

/* ------------------------------------------------------ SegmentedControl */

export interface SegmentedProps {
  value: string;
  onValueChange: (value: string) => void;
  options: SelectOption[];
  label: string;
  /** Spread across the full width (labels never wrap; scrolls sideways if they cannot fit). */
  fill?: boolean;
}

export function Segmented({ value, onValueChange, options, label, fill }: SegmentedProps) {
  return (
    <RToggleGroup.Root
      type="single"
      value={value}
      aria-label={label}
      // A segmented control always has a selection; ignore the "deselect" event.
      onValueChange={(v) => v && onValueChange(v)}
      className={clsx("rounded-control bg-fill p-0.5", fill ? "flex w-full overflow-x-auto" : "inline-flex")}
    >
      {options.map((o) => (
        <RToggleGroup.Item
          key={o.value}
          value={o.value}
          className={clsx(
            "h-6 whitespace-nowrap rounded-[5px] text-body font-medium text-fg-2",
            fill ? "flex-1 px-2" : "px-3",
            "transition-[background-color,color,box-shadow] duration-150 ease-standard",
            "data-[state=on]:bg-surface-raised data-[state=on]:text-fg data-[state=on]:shadow-card",
          )}
        >
          {o.label}
        </RToggleGroup.Item>
      ))}
    </RToggleGroup.Root>
  );
}

/* ------------------------------------------------------------- TextField */

export interface TextFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: string;
  error?: string;
}

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(
  { label, hint, error, className, id, ...rest },
  ref,
) {
  const auto = useId();
  const inputId = id ?? auto;
  const descId = `${inputId}-desc`;
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={inputId} className="text-footnote font-medium text-fg-2">
        {label}
      </label>
      <input
        ref={ref}
        id={inputId}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? descId : undefined}
        className={clsx(
          "h-9 select-text rounded-card bg-fill px-3 text-headline text-fg placeholder:text-fg-3",
          "transition-shadow duration-150 ease-standard",
          error && "shadow-[inset_0_0_0_1.5px_var(--danger)]",
          className,
        )}
        {...rest}
      />
      {(error || hint) && (
        <p id={descId} className={clsx("text-footnote", error ? "text-danger" : "text-fg-2")}>
          {error ?? hint}
        </p>
      )}
    </div>
  );
});
