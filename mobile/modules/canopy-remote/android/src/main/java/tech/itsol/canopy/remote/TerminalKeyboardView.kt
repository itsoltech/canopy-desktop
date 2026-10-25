package tech.itsol.canopy.remote

import android.content.Context
import android.view.KeyEvent
import expo.modules.kotlin.AppContext
import expo.modules.kotlin.views.ExpoView
import expo.modules.kotlin.viewevent.EventDispatcher

class TerminalKeyboardView(context: Context, appContext: AppContext) : ExpoView(context, appContext) {
  val onTerminalKey by EventDispatcher()
  init {
    isFocusable = true
    isFocusableInTouchMode = true
    contentDescription = "Terminal hardware keyboard"
    setOnClickListener { requestFocus() }
  }
  override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
    val special = mapOf(
      KeyEvent.KEYCODE_DPAD_UP to "up", KeyEvent.KEYCODE_DPAD_DOWN to "down", KeyEvent.KEYCODE_DPAD_LEFT to "left", KeyEvent.KEYCODE_DPAD_RIGHT to "right",
      KeyEvent.KEYCODE_ESCAPE to "escape", KeyEvent.KEYCODE_DEL to "backspace", KeyEvent.KEYCODE_FORWARD_DEL to "delete",
      KeyEvent.KEYCODE_MOVE_HOME to "home", KeyEvent.KEYCODE_MOVE_END to "end", KeyEvent.KEYCODE_PAGE_UP to "pageup", KeyEvent.KEYCODE_PAGE_DOWN to "pagedown",
      KeyEvent.KEYCODE_ENTER to "enter", KeyEvent.KEYCODE_TAB to "tab", KeyEvent.KEYCODE_INSERT to "insert")
    val key = special[keyCode] ?: if (keyCode in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12) "f${keyCode - KeyEvent.KEYCODE_F1 + 1}"
      else event.getUnicodeChar(0).takeIf { it > 0 }?.let { String(Character.toChars(it)) }
    if (key == null) return super.onKeyDown(keyCode, event)
    onTerminalKey(mapOf("key" to key, "ctrl" to event.isCtrlPressed, "alt" to event.isAltPressed, "shift" to event.isShiftPressed, "meta" to event.isMetaPressed))
    return true
  }
}
