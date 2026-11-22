// Ported from Sowel ui/src/components/devices/DeviceNameEditor.tsx (Sowel commit 5da84dc1, UI 1.87.0).
// Changed for a list of rows (spec 017): the name is a `span` of the body size, not an `h1` of the
// page, and the input has the body size.
import { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Check, X } from "lucide-react";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";

interface DeviceNameEditorProps {
  name: string;
  onSave: (name: string) => Promise<void>;
}

export function DeviceNameEditor({ name, onSave }: DeviceNameEditorProps) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(name);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editing]);

  useEffect(() => {
    setValue(name);
  }, [name]);

  const handleSave = async () => {
    const trimmed = value.trim();
    if (!trimmed || trimmed === name) {
      setEditing(false);
      setValue(name);
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await onSave(trimmed);
      setEditing(false);
    } catch (err) {
      setError(apiErrorStatus(err) === null ? t("devices.saveFailed") : apiErrorText(err, t));
      setValue(name);
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    setEditing(false);
    setValue(name);
    setError(null);
  };

  const handleKeyDown = (e: { key: string }) => {
    if (e.key === "Enter") handleSave();
    if (e.key === "Escape") handleCancel();
  };

  if (!editing) {
    return (
      <div className="flex items-center gap-2 group">
        <span className="text-small font-medium text-text truncate">{name}</span>
        <button
          onClick={() => setEditing(true)}
          className="p-1.5 rounded-sm text-text-tertiary opacity-0 group-hover:opacity-100 hover:text-text-secondary hover:bg-border-light transition-all duration-150 ease-out"
          title={t("devices.editName")}
        >
          <Pencil size={14} strokeWidth={1.5} />
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(e) => setValue((e.target as HTMLInputElement).value)}
          onKeyDown={handleKeyDown}
          disabled={saving}
          className="text-small text-text bg-transparent border-b-2 border-primary outline-none px-0 py-0 w-auto min-w-[160px]"
        />
        <button
          onClick={handleSave}
          disabled={saving}
          className="p-1.5 rounded-sm text-success hover:bg-success/10 transition-colors duration-150 ease-out disabled:opacity-50"
          title={t("common.save")}
        >
          <Check size={16} strokeWidth={1.5} />
        </button>
        <button
          onClick={handleCancel}
          disabled={saving}
          className="p-1.5 rounded-sm text-text-tertiary hover:text-error hover:bg-error/10 transition-colors duration-150 ease-out disabled:opacity-50"
          title={t("common.cancel")}
        >
          <X size={16} strokeWidth={1.5} />
        </button>
      </div>
      {error && <span className="text-caption text-error">{error}</span>}
    </div>
  );
}
