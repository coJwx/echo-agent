interface ComingSoonProps {
  title: string;
  description: string;
  hint?: string;
}

export default function ComingSoon({
  title,
  description,
  hint,
}: ComingSoonProps) {
  return (
    <div className="flex-1 flex items-center justify-center p-6">
      <div className="card p-6 max-w-md text-center">
        <div className="text-5xl mb-4">🚧</div>
        <h2 className="text-xl font-semibold text-ink-primary mb-3">{title}</h2>
        <p className="text-[14px] leading-[1.5] text-ink-secondary">{description}</p>
        {hint && (
          <p className="mt-3 rounded-lg bg-bg-panel px-3 py-2 text-[12px] leading-[1.45] text-ink-secondary">
            {hint}
          </p>
        )}
      </div>
    </div>
  );
}
