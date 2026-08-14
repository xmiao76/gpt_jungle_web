import { useEffect, type ReactNode } from "react";

interface ModalProps {
  title: string;
  children: ReactNode;
  onClose?: () => void;
  wide?: boolean;
  labelledBy?: string;
}

export function Modal({ title, children, onClose, wide = false, labelledBy = "modal-title" }: ModalProps) {
  useEffect(() => {
    if (!onClose) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose?.();
    }}>
      <section
        className={`modal-card${wide ? " modal-card--wide" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
      >
        <header className="modal-header">
          <div>
            <span className="eyebrow">Jungle / Dou Shou Qi</span>
            <h2 id={labelledBy}>{title}</h2>
          </div>
          {onClose ? (
            <button className="icon-button" type="button" onClick={onClose} aria-label="Close dialog">
              ×
            </button>
          ) : null}
        </header>
        <div className="modal-content">{children}</div>
      </section>
    </div>
  );
}

