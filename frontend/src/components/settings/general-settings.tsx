"use client";

import { useState, useEffect } from "react";
import { useSettings, ColorMode } from "@/lib/use-settings";
import { Moon, Sun, Monitor } from "lucide-react";

export function GeneralSettings() {
  const { settings, updateSetting } = useSettings();
  const [localSettings, setLocalSettings] = useState(settings);

  useEffect(() => {
    setLocalSettings(settings);
  }, [settings]);

  const handleSave = () => {
    Object.keys(localSettings).forEach((key) => {
      const typedKey = key as keyof typeof localSettings;
      if (localSettings[typedKey] !== settings[typedKey]) {
        updateSetting(typedKey, localSettings[typedKey]);
      }
    });
  };

  const colorModes: Array<{ value: ColorMode; label: string; icon: React.ComponentType<{ className?: string }> }> = [
    { value: "light", label: "Light", icon: Sun },
    { value: "auto", label: "Auto", icon: Monitor },
    { value: "dark", label: "Dark", icon: Moon },
  ];

  return (
    <div className="space-y-8">
      {/* Profile Section */}
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-6">Profile</h3>
        <div className="space-y-6">
          <div>
            <label htmlFor="full-name" className="block text-sm font-medium text-foreground mb-2">
              Full name
            </label>
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-full bg-muted flex items-center justify-center text-muted-foreground font-semibold">
                {localSettings.fullName?.[0]?.toUpperCase() || "H"}
              </div>
              <input
                id="full-name"
                type="text"
                value={localSettings.fullName}
                onChange={(e) => setLocalSettings({ ...localSettings, fullName: e.target.value })}
                className="flex-1 px-5 py-3 border border-input rounded-full shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:border-ring text-sm bg-background text-foreground"
                placeholder="Enter your full name"
              />
            </div>
          </div>

          <div>
            <label htmlFor="preferred-name" className="block text-sm font-medium text-foreground mb-2">
              Preferred name
            </label>
            <input
              id="preferred-name"
              type="text"
              value={localSettings.preferredName}
              onChange={(e) => setLocalSettings({ ...localSettings, preferredName: e.target.value })}
              className="w-full px-5 py-3 border border-input rounded-full shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:border-ring text-sm bg-background text-foreground"
              placeholder="Enter your preferred name"
            />
          </div>

          <div>
            <label htmlFor="work-function" className="block text-sm font-medium text-foreground mb-2">
              What best describes your work?
            </label>
            <select
              id="work-function"
              value={localSettings.workFunction}
              onChange={(e) => setLocalSettings({ ...localSettings, workFunction: e.target.value })}
              className="w-full px-5 py-3 border border-input rounded-full shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:border-ring text-sm bg-background text-foreground"
            >
              <option value="">Select your work function</option>
              <option value="developer">Developer</option>
              <option value="designer">Designer</option>
              <option value="product-manager">Product Manager</option>
              <option value="researcher">Researcher</option>
              <option value="writer">Writer</option>
              <option value="student">Student</option>
              <option value="other">Other</option>
            </select>
          </div>

          <div>
            <label htmlFor="personal-preferences" className="block text-sm font-medium text-foreground mb-2">
              Personal preferences
            </label>
            <textarea
              id="personal-preferences"
              value={localSettings.personalPreferences}
              onChange={(e) => setLocalSettings({ ...localSettings, personalPreferences: e.target.value })}
              rows={4}
              className="w-full px-5 py-4 border border-input rounded-3xl shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:border-ring text-sm bg-background text-foreground"
              placeholder="e.g. keep explanations brief and to the point."
            />
            <p className="mt-2 text-sm text-muted-foreground">
              Your preferences will apply to all conversations.
            </p>
          </div>
        </div>
      </div>

      {/* Appearance Section */}
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-4">Appearance</h3>
        <div>
          <label className="block text-sm font-medium text-foreground mb-3">Color mode</label>
          <div className="grid grid-cols-3 gap-3">
            {colorModes.map((mode) => {
              const Icon = mode.icon;
              const isSelected = localSettings.colorMode === mode.value;
              return (
                <button
                  key={mode.value}
                  type="button"
                  onClick={() => setLocalSettings({ ...localSettings, colorMode: mode.value })}
                  className={`flex flex-col items-center gap-2 p-5 border-2 rounded-2xl transition-colors ${
                    isSelected
                      ? "border-primary bg-primary/10"
                      : "border-border hover:border-muted-foreground bg-background"
                  }`}
                >
                  <Icon className="w-6 h-6 text-muted-foreground" />
                  <span className="text-sm font-medium text-foreground">{mode.label}</span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Incognito Mode Section */}
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-4">Privacy</h3>
        <div className="flex items-center justify-between">
          <div className="flex-1">
            <p className="text-sm font-medium text-foreground">Incognito mode</p>
            <p className="text-sm text-muted-foreground mt-1">
              When enabled, your conversation history will not be saved. This helps maintain privacy for sensitive
              conversations.
            </p>
          </div>
          <button
            type="button"
            onClick={() => setLocalSettings({ ...localSettings, incognitoMode: !localSettings.incognitoMode })}
            className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background ${
              localSettings.incognitoMode ? "bg-primary" : "bg-input"
            }`}
          >
            <span
              className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-background shadow ring-0 transition duration-200 ease-in-out ${
                localSettings.incognitoMode ? "translate-x-5" : "translate-x-0"
              }`}
            />
          </button>
        </div>
      </div>

      {/* Save Button */}
      <div className="flex justify-end pt-4 border-t border-border">
        <button
          type="button"
          onClick={handleSave}
          className="px-6 py-3 text-sm font-medium text-primary-foreground bg-primary rounded-full hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background transition-colors"
        >
          Save changes
        </button>
      </div>
    </div>
  );
}
