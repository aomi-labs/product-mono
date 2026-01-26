"use client";

import { useEffect } from "react";
import { useSettings } from "@/lib/use-settings";

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const { settings } = useSettings();

  // Theme is applied automatically by useSettings hook
  // This component just ensures the hook is initialized
  return <>{children}</>;
}
