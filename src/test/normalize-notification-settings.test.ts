import { describe, it, expect } from "vitest";
import {
  normalizeNotificationSettings,
  DEFAULT_NOTIFICATION_SETTINGS,
} from "../types";

describe("normalizeNotificationSettings", () => {
  it("returns defaults for null", () => {
    expect(normalizeNotificationSettings(null)).toEqual(DEFAULT_NOTIFICATION_SETTINGS);
  });

  it("returns defaults for non-object", () => {
    expect(normalizeNotificationSettings("oops")).toEqual(DEFAULT_NOTIFICATION_SETTINGS);
  });

  it("{} normalizes to full defaults with enabled=true", () => {
    const result = normalizeNotificationSettings({});
    expect(result).toEqual(DEFAULT_NOTIFICATION_SETTINGS);
    expect(result.enabled).toBe(true);
  });

  it("preserves explicit false while other fields fall back to defaults", () => {
    const result = normalizeNotificationSettings({
      enabled: false,
      types: { done: false },
    });
    expect(result.enabled).toBe(false);
    expect(result.inApp).toBe(true);
    expect(result.system).toBe(true);
    expect(result.sound).toBe(true);
    expect(result.types.done).toBe(false);
    expect(result.types.failed).toBe(true);
    expect(result.types.idle).toBe(true);
    expect(result.toastPosition).toBe("bottom-right");
  });

  it("normalizes invalid toastPosition to default", () => {
    const result = normalizeNotificationSettings({ toastPosition: "middle" });
    expect(result.toastPosition).toBe(DEFAULT_NOTIFICATION_SETTINGS.toastPosition);
  });

  it("preserves valid toastPosition", () => {
    const result = normalizeNotificationSettings({ toastPosition: "top-left" });
    expect(result.toastPosition).toBe("top-left");
  });

  it("ignores non-boolean values, falling back to defaults", () => {
    const result = normalizeNotificationSettings({
      enabled: "yes",
      inApp: 1,
      system: null,
      sound: undefined,
    });
    expect(result.enabled).toBe(true);
    expect(result.inApp).toBe(true);
    expect(result.system).toBe(true);
    expect(result.sound).toBe(true);
  });

  it("preserves all explicit boolean values", () => {
    const result = normalizeNotificationSettings({
      enabled: false,
      inApp: false,
      system: false,
      sound: false,
      types: { done: false, failed: false, idle: false },
    });
    expect(result.enabled).toBe(false);
    expect(result.inApp).toBe(false);
    expect(result.system).toBe(false);
    expect(result.sound).toBe(false);
    expect(result.types).toEqual({ done: false, failed: false, idle: false });
  });
});
