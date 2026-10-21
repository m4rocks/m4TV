import type { ReactNode } from "react";

export const buttonPrimary = "rounded bg-blue-600 px-4 py-3 text-white disabled:opacity-50";
export const input = "rounded border p-3 text-lg";
export const listButton = "rounded bg-gray-200 p-3 text-left text-lg";

/** An error the viewer should see: a refused command, a failed pairing. */
export function ErrorBanner({ children }: { children: ReactNode }) {
  return (
    <p role="alert" className="rounded bg-red-100 p-3 text-red-800">
      {children}
    </p>
  );
}

/** Something the viewer should know that is not an error: the link, or what the TV said. */
export function StatusBanner({ children }: { children: ReactNode }) {
  return (
    <p role="status" className="rounded bg-yellow-100 p-3 text-yellow-900">
      {children}
    </p>
  );
}
