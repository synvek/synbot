/**
 * Intents that cross the shell without a context: the palette and the workspace chip can ask the
 * sidebar to reveal its 「打开文件夹」 form without either of them owning the picker or its errors.
 */
export const OPEN_FOLDER_EVENT = 'synbot:open-folder'

export function requestOpenFolder(): void {
  window.dispatchEvent(new Event(OPEN_FOLDER_EVENT))
}
