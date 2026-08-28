import { useState, useEffect } from 'react';
import { listen, UnlistenFn, Event } from '@tauri-apps/api/event';

export interface DragDropResult {
  isDragging: boolean;
}

/**
 * File drag-and-drop for the app window.
 *
 * In Tauri the webview's native drag handling emits the v2 event names
 * `tauri://drag-over` / `tauri://drag-drop` / `tauri://drag-leave`, whose drop
 * payload carries the real filesystem paths. In a plain browser (dev) we fall
 * back to DOM drag events that supply `File` objects.
 */
export function useDragDrop(
  onPathsDropped?: (paths: string[]) => void,
  onBrowserFilesDropped?: (files: File[]) => void
): DragDropResult {
  const [isDragging, setIsDragging] = useState(false);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      unlisteners.push(await listen('tauri://drag-over', () => setIsDragging(true)));
      unlisteners.push(await listen('tauri://drag-enter', () => setIsDragging(true)));
      unlisteners.push(
        await listen('tauri://drag-leave', () => setIsDragging(false))
      );
      unlisteners.push(
        await listen('tauri://drag-drop', (event: Event<{ paths?: string[] } | string[]>) => {
          setIsDragging(false);
          const payload = event.payload ?? [];
          const paths = Array.isArray(payload)
            ? payload.map(String)
            : Array.isArray(payload.paths)
              ? payload.paths
              : [];
          if (paths.length > 0) onPathsDropped?.(paths);
        })
      );
    };
    void setup();

    // Browser-dev fallback: plain DOM drag events hand us real File objects.
    const handleWindowDragOver = (event: DragEvent) => {
      if (event.dataTransfer && event.dataTransfer.types.includes('Files')) {
        event.preventDefault();
        setIsDragging(true);
      }
    };

    const handleWindowDrop = (event: DragEvent) => {
      if (event.dataTransfer && event.dataTransfer.files.length > 0) {
        event.preventDefault();
        setIsDragging(false);
        onBrowserFilesDropped?.(Array.from(event.dataTransfer.files));
      }
    };

    const handleWindowDragLeave = (event: DragEvent) => {
      if (event.target === window) {
        setIsDragging(false);
      }
    };

    window.addEventListener('dragover', handleWindowDragOver);
    window.addEventListener('drop', handleWindowDrop);
    window.addEventListener('dragleave', handleWindowDragLeave);

    return () => {
      unlisteners.forEach((u) => u?.());
      window.removeEventListener('dragover', handleWindowDragOver);
      window.removeEventListener('drop', handleWindowDrop);
      window.removeEventListener('dragleave', handleWindowDragLeave);
    };
  }, [onPathsDropped, onBrowserFilesDropped]);

  return { isDragging };
}