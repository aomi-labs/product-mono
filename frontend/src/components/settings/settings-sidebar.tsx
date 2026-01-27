"use client";

import { Settings, Key } from "lucide-react";

export type SettingsCategory = "general" | "api-keys";

interface SettingsSidebarProps {
  activeCategory: SettingsCategory;
  onCategoryChange: (category: SettingsCategory) => void;
}

const categories: Array<{
  id: SettingsCategory;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}> = [
  { id: "general", label: "General", icon: Settings },
  { id: "api-keys", label: "API Keys", icon: Key },
];

export function SettingsSidebar({ activeCategory, onCategoryChange }: SettingsSidebarProps) {

  return (
    <div className="w-64 border-r border-gray-200 bg-white h-full overflow-y-auto">
      <div className="p-4">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Settings</h2>
        <nav className="space-y-1">
          {categories.map((category) => {
            const Icon = category.icon;
            const isActive = activeCategory === category.id;
            return (
              <button
                key={category.id}
                onClick={() => onCategoryChange(category.id)}
                className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                  isActive
                    ? "bg-gray-100 text-gray-900"
                    : "text-gray-700 hover:bg-gray-50 hover:text-gray-900"
                }`}
              >
                <Icon className="w-5 h-5" />
                <span>{category.label}</span>
              </button>
            );
          })}
        </nav>
      </div>
    </div>
  );
}
