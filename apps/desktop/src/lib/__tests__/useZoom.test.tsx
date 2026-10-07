import { fireEvent, render, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'
import { useZoom } from '@/lib/appearance'
import { useUi } from '@/stores/ui'
import { mockBackend } from '@/test/tauri'

function Harness() {
  useZoom()
  return <input aria-label="field" />
}

afterEach(() => {
  useUi.setState({ zoom: 1 })
  document.documentElement.style.zoom = ''
})

describe('useZoom', () => {
  it('zooms with Ctrl + = / - / 0, even while typing, using CSS zoom outside the app', async () => {
    const user = userEvent.setup()
    const { getByLabelText } = render(<Harness />)
    getByLabelText('field').focus()
    await user.keyboard('{Control>}={/Control}')
    expect(useUi.getState().zoom).toBe(1.1)
    await waitFor(() => expect(document.documentElement.style.zoom).toBe('1.1'))
    await user.keyboard('{Meta>}={/Meta}')
    expect(useUi.getState().zoom).toBe(1.2)
    await user.keyboard('{Control>}-{/Control}')
    expect(useUi.getState().zoom).toBe(1.1)
    await user.keyboard('{Control>}0{/Control}')
    expect(useUi.getState().zoom).toBe(1)
    await waitFor(() => expect(document.documentElement.style.zoom).toBe(''))
    // Plain keys are just typing.
    await user.keyboard('=-0')
    expect(useUi.getState().zoom).toBe(1)
  })

  it('zooms with Ctrl + wheel, one step per notch', () => {
    render(<Harness />)
    fireEvent.wheel(window, { deltaY: -100, ctrlKey: true })
    expect(useUi.getState().zoom).toBe(1.1)
    fireEvent.wheel(window, { deltaY: 100, ctrlKey: true })
    fireEvent.wheel(window, { deltaY: 100, ctrlKey: true })
    expect(useUi.getState().zoom).toBe(0.9)
    // Small trackpad deltas add up; a wheel without Ctrl only scrolls.
    fireEvent.wheel(window, { deltaY: -30, ctrlKey: true })
    expect(useUi.getState().zoom).toBe(0.9)
    fireEvent.wheel(window, { deltaY: -30, ctrlKey: true })
    expect(useUi.getState().zoom).toBe(1)
    fireEvent.wheel(window, { deltaY: -100 })
    expect(useUi.getState().zoom).toBe(1)
  })

  it('uses the webview zoom in the app', async () => {
    const backend = mockBackend({ 'plugin:webview|set_webview_zoom': null })
    useUi.setState({ zoom: 1.3 })
    render(<Harness />)
    await waitFor(() => expect(backend.argsOf('plugin:webview|set_webview_zoom')).toEqual([{ label: 'main', value: 1.3 }]))
    expect(document.documentElement.style.zoom).toBe('')
  })

  it('falls back to CSS zoom if the webview refuses', async () => {
    mockBackend({ 'plugin:webview|set_webview_zoom': () => Promise.reject('not allowed') })
    useUi.setState({ zoom: 0.9 })
    render(<Harness />)
    await waitFor(() => expect(document.documentElement.style.zoom).toBe('0.9'))
  })
})
