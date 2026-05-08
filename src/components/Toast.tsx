import { createContext, useContext, useState, useCallback, useRef } from "react";
import { CheckCircle2, AlertCircle, AlertTriangle } from "lucide-react";
import type React from "react";

type ToastType = "error" | "warning" | "success" | "info";

interface ToastItem {
  id: string;
  message: string;
  type: ToastType;
  onClick?: () => void;
}

interface ToastContextValue {
  showToast: (message: string, type?: ToastType, onClick?: () => void) => void;
}

const ToastContext = createContext<ToastContextValue>({ showToast: () => {} });

export function useToast() {
  return useContext(ToastContext);
}

export type { ToastType };

function toastColor(type: ToastType): string {
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
  const size = 14;
  const color = "var(--fg-on-accent)";
  switch (type) {
    case "error":
      return <AlertCircle size={size} color={color} />;
    case "warning":
      return <AlertTriangle size={size} color={color} />;
    case "success":
      return <CheckCircle2 size={size} color={color} />;
    case "info":
      return <AlertCircle size={size} color={color} />;
  }
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timerMap = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const showToast = useCallback(
    (message: string, type: ToastType = "error", onClick?: () => void) => {
      const id = `${Date.now()}-${Math.random()}`;
      setToasts((prev) => [...prev.slice(-2), { id, message, type, onClick }]);
      const timer = setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
        timerMap.current.delete(id);
      }, 5000);
      timerMap.current.set(id, timer);
    },
    [],
  );

  const dismiss = useCallback((id: string) => {
    const timer = timerMap.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timerMap.current.delete(id);
    }
    setToasts((prev) => prev.filter((t) => t.id !== id));
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
    <div
      style={{
        position: "fixed",
        bottom: 20,
        right: 20,
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxWidth: 380,
        pointerEvents: "none",
      }}
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          onClick={() => {
            if (t.onClick) t.onClick();
            onDismiss(t.id);
          }}
          className="toast-item"
          style={{
            pointerEvents: "auto",
            display: "flex",
            alignItems: "flex-start",
            gap: 10,
            padding: "10px 12px 10px 14px",
            borderRadius: 10,
            background: toastColor(t.type),
            color: "var(--fg-on-accent)",
            fontSize: 12.5,
            fontWeight: 500,
            boxShadow: "var(--shadow-toast)",
            lineHeight: 1.5,
            cursor: t.onClick ? "pointer" : "default",
          }}
        >
          <div style={{ flexShrink: 0, marginTop: 1 }}>
            <ToastIcon type={t.type} />
          </div>
          <span style={{ flex: 1 }}>{t.message}</span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDismiss(t.id);
            }}
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "var(--inverse-muted)",
              padding: "0 0 0 4px",
              fontSize: 18,
              lineHeight: 1,
              flexShrink: 0,
              fontFamily: "inherit",
            }}
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
