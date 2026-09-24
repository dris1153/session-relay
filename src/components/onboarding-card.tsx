import type { ReactNode } from "react";
import { t } from "../lib/i18n";
import { LanguageSwitch } from "./language-switch";

/** Centered editorial card on the parchment canvas, with Clay step dots. */
export function OnboardingCard({ step, total, title, lead, children }: { step: number; total: number; title: string; lead?: string; children: ReactNode }) {
  return (
    <main className="flex min-h-screen items-center justify-center px-6 py-16">
      <LanguageSwitch />
      <section className="w-full max-w-[560px] rounded-card-elevated bg-paper-white p-8">
        <div className="mb-8 flex gap-2" aria-hidden="true">
          {Array.from({ length: total }, (_, i) => (
            <span key={i} className={`h-1.5 w-6 rounded-full ${i < step ? "bg-clay" : "bg-chalk"}`} />
          ))}
        </div>
        {step > 0 && <p className="sr-only">{t("common.step", { step, total })}</p>}
        <h1 className="font-serif text-heading font-normal text-carbon-ink">{title}</h1>
        {lead && <p className="mt-3 text-[16px] leading-relaxed text-ashen">{lead}</p>}
        <div className="mt-8 flex flex-col gap-6">{children}</div>
      </section>
    </main>
  );
}

export function ErrorNote({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <p role="alert" className="rounded-control bg-soft-stone px-4 py-3 text-body text-carbon-ink">
      {message}
    </p>
  );
}
