type Props = {
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  secondaryLabel?: string;
  onSecondary?: () => void;
};

export function ConfirmBar({
  message,
  confirmLabel = "Yes",
  cancelLabel = "Cancel",
  onConfirm,
  onCancel,
  secondaryLabel,
  onSecondary,
}: Props) {
  return (
    <div className="confirm-bar" role="alertdialog">
      <span>{message}</span>
      <div className="row-actions">
        <button type="button" className="btn" onClick={onConfirm}>
          {confirmLabel}
        </button>
        {secondaryLabel && onSecondary && (
          <button type="button" onClick={onSecondary}>
            {secondaryLabel}
          </button>
        )}
        <button type="button" onClick={onCancel}>
          {cancelLabel}
        </button>
      </div>
    </div>
  );
}
