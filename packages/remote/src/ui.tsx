import type { ReactNode } from "react";

export const buttonPrimary = "rounded bg-ink px-4 py-3 font-bold text-background disabled:opacity-50";
export const input = "rounded border border-border bg-surface p-3 text-lg text-text";
export const listButton = "rounded border border-border bg-surface p-3 text-left text-lg";

/** An error the viewer should see: a refused command, a failed pairing. */
export function ErrorBanner({ children }: { children: ReactNode }) {
  return (
    <p role="alert" className="flex gap-2 rounded border border-state bg-surface p-3 text-state">
      <span aria-hidden className="font-bold">
        !
      </span>
      <span>{children}</span>
    </p>
  );
}

/** Something the viewer should know that is not an error: the link, or what the TV said. */
export function StatusBanner({ children }: { children: ReactNode }) {
  return (
    <p role="status" className="rounded border border-border bg-raised p-3 text-text">
      {children}
    </p>
  );
}
