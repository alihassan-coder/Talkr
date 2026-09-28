export function HistoryPage() {
  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <header className="space-y-2">
        <h1 className="text-4xl font-semibold tracking-tight text-text-primary">History</h1>
        <p className="text-text-secondary">Your past generations and transcriptions</p>
      </header>

      <div className="bg-card rounded-[var(--radius-card)] p-6 shadow-[var(--shadow-card)] border border-border">
        <p className="text-text-secondary text-center py-12">History will appear here</p>
      </div>
    </div>
  )
}