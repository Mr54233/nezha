import { createContext, useContext, useState, useCallback, useRef, useEffect } from "react";
import { CheckCircle2, AlertCircle, AlertTriangle, Info } from "lucide-react";
import type React from "react";

type ToastType = "error" | "warning" | "success" | "info";

interface ToastItem {
  id: string;
  message: string;
  type: ToastType;
  onClick?: () => void;
  exiting?: boolean;
}

interface ToastContextValue {
  showToast: (message: string, type?: ToastType, onClick?: () => void) => void;
}

const ToastContext = createContext<ToastContextValue>({ showToast: () => {} });

export function useToast() {
  return useContext(ToastContext);
}

export type { ToastType };

function toastAccentColor(type: ToastType): string {
  switch (type) {
    case "error":
      return "var(--danger)";
    case "warning":
      return "var(--warning)";
    case "success":
      return "var(--success)";
    case "info":
      return "var(--accent)";
  }
}

function ToastIcon({ type }: { type: ToastType }) {
  const size = 16;
  const color = toastAccentColor(type);
  switch (type) {
    case "error":
      return <AlertCircle size={size} color={color} />;
    case "warning":
      return <AlertTriangle size={size} color={color} />;
    case "success":
      return <CheckCircle2 size={size} color={color} />;
    case "info":
      return <Info size={size} color={color} />;
  }
}

const DURATION = 4500;

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timerMap = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: string) => {
    const timer = timerMap.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timerMap.current.delete(id);
    }
    setToasts((prev) => prev.map((t) => t.id === id ? { ...t, exiting: true } : t));
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 200);
  }, []);

  const showToast = useCallback(
    (message: string, type: ToastType = "error", onClick?: () => void) => {
      const id = `${Date.now()}-${Math.random()}`;
      setToasts((prev) => [...prev.slice(-2), { id, message, type, onClick }]);
      const timer = setTimeout(() => dismiss(id), DURATION);
      timerMap.current.set(id, timer);
    },
    [dismiss],
  );

  useEffect(() => {
    return () => {
      timerMap.current.forEach((timer) => clearTimeout(timer));
    };
  }, []);

  return (
    <ToastContext.Provider value={{ showToast }}>
      {children}
      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}

function ToastContainer({
  toasts,
  onDismiss,
}: {
  toasts: ToastItem[];
  onDismiss: (id: string) => void;
}) {
  if (toasts.length === 0) return null;
  return (
    <div style={{
      position: "fixed",
      bottom: 20,
      right: 20,
      zIndex: 9999,
      display: "flex",
      flexDirection: "column",
      gap: 10,
      width: 360,
      pointerEvents: "none",
    }}>
      {toasts.map((t) => (
        <div
          key={t.id}
          onClick={() => {
            if (t.onClick) t.onClick();
            onDismiss(t.id);
          }}
          style={{
            pointerEvents: "auto",
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "12px 14px",
            borderRadius: "var(--radius-lg)",
            background: "var(--bg-card)",
            border: "1px solid var(--border-medium)",
            borderLeft: `3px solid ${toastAccentColor(t.type)}`,
            boxShadow: "var(--shadow-md)",
            animation: t.exiting
              ? "toast-out 0.2s ease forwards"
              : "toast-in 0.25s ease forwards",
            transition: "box-shadow 0.15s ease, transform 0.15s ease",
            cursor: t.onClick ? "pointer" : "default",
            position: "relative",
            overflow: "hidden",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.boxShadow = "var(--shadow-popover)";
            if (t.onClick) e.currentTarget.style.transform = "translateY(-1px)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.boxShadow = "var(--shadow-md)";
            e.currentTarget.style.transform = "none";
          }}
        >
          <div style={{ flexShrink: 0, display: "flex", alignItems: "center" }}>
            <ToastIcon type={t.type} />
          </div>
          <span style={{
            flex: 1,
            fontSize: 12.5,
            fontWeight: 500,
            lineHeight: 1.5,
            color: "var(--text-primary)",
          }}>
            {t.message}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDismiss(t.id);
            }}
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "var(--text-hint)",
              padding: "2px",
              fontSize: 16,
              lineHeight: 1,
              flexShrink: 0,
              fontFamily: "inherit",
              borderRadius: 4,
              transition: "color 0.15s ease",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = "var(--text-secondary)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = "var(--text-hint)";
            }}
          >
            ×
          </button>
          {/* progress bar */}
          {!t.exiting && (
            <div style={{
              position: "absolute",
              bottom: 0,
              left: 0,
              height: 2,
              background: toastAccentColor(t.type),
              opacity: 0.3,
              borderRadius: "0 0 0 var(--radius-lg)",
              animation: `toast-progress ${DURATION}ms linear forwards`,
            }} />
          )}
        </div>
      ))}
    </div>
  );
}
