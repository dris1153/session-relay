import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost";

const styles: Record<Variant, string> = {
  // DESIGN.md: actions stay dark on light; Clay is never a button fill.
  primary: "bg-carbon-ink text-bone-parchment hover:bg-graphite",
  secondary: "border border-chalk bg-paper-white text-graphite hover:border-mist hover:text-carbon-ink",
  ghost: "text-graphite hover:text-carbon-ink",
};

export function Button({ variant = "primary", className = "", ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      className={`rounded-control px-5 py-2 text-[15px] font-medium transition-colors duration-200 disabled:opacity-50 ${styles[variant]} ${className}`}
      {...props}
    />
  );
}
