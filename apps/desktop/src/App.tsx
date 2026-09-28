import { Routes, Route } from 'react-router-dom'
import { Sidebar } from './components/Sidebar'
import { SpeakPage } from './pages/SpeakPage'
import { TranscribePage } from './pages/TranscribePage'
import { ModelsPage } from './pages/ModelsPage'
import { HistoryPage } from './pages/HistoryPage'
import { SettingsPage } from './pages/SettingsPage'

export function App() {
  return (
    <div className="flex h-screen w-screen bg-app-canvas overflow-hidden">
      <Sidebar />
      <main className="flex-1 overflow-y-auto p-8">
        <Routes>
          <Route path="/" element={<SpeakPage />} />
          <Route path="/speak" element={<SpeakPage />} />
          <Route path="/transcribe" element={<TranscribePage />} />
          <Route path="/models" element={<ModelsPage />} />
          <Route path="/history" element={<HistoryPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Routes>
      </main>
    </div>
  )
}