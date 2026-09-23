import type { InputHTMLAttributes } from "react";

export function TextField({ label, hint, className = "", ...props }: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: string }) {
  return (
    <label className="flex flex-col gap-2">
      <span className="text-body font-medium text-graphite">{label}</span>
      <input
        className={`rounded-control border border-mist bg-paper-white px-3 py-2 text-[15px] text-carbon-ink outline-none transition-shadow focus:border-graphite focus:shadow-soft ${className}`}
        {...props}
      />
      {hint && <span className="text-caption text-ashen">{hint}</span>}
    </label>
  );
}
