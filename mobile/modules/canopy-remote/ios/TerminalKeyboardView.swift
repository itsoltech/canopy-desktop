import ExpoModulesCore
import UIKit

// This view captures hardware keys only. The ordinary text field owns IME input.
final class TerminalKeyboardView: ExpoView {
  let onTerminalKey = EventDispatcher()
  override var canBecomeFirstResponder: Bool { true }
  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    isAccessibilityElement = true
    accessibilityLabel = "Terminal hardware keyboard"
    addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(focusKeyboard)))
  }
  @objc private func focusKeyboard() { becomeFirstResponder() }
  override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
    var consumed = Set<UIPress>()
    for press in presses {
      guard let key = press.key else { continue }
      let special: [UIKeyboardHIDUsage: String] = [
        .keyboardUpArrow: "up", .keyboardDownArrow: "down", .keyboardLeftArrow: "left", .keyboardRightArrow: "right",
        .keyboardEscape: "escape", .keyboardDeleteOrBackspace: "backspace", .keyboardDeleteForward: "delete",
        .keyboardHome: "home", .keyboardEnd: "end", .keyboardPageUp: "pageup", .keyboardPageDown: "pagedown",
        .keyboardReturnOrEnter: "enter", .keyboardTab: "tab", .keyboardInsert: "insert",
        .keyboardF1: "f1", .keyboardF2: "f2", .keyboardF3: "f3", .keyboardF4: "f4", .keyboardF5: "f5", .keyboardF6: "f6",
        .keyboardF7: "f7", .keyboardF8: "f8", .keyboardF9: "f9", .keyboardF10: "f10", .keyboardF11: "f11", .keyboardF12: "f12"
      ]
      let value = special[key.keyCode] ?? key.charactersIgnoringModifiers
      guard !value.isEmpty else { continue }
      onTerminalKey(["key": value, "ctrl": key.modifierFlags.contains(.control), "alt": key.modifierFlags.contains(.alternate), "shift": key.modifierFlags.contains(.shift), "meta": key.modifierFlags.contains(.command)])
      consumed.insert(press)
    }
    let remainder = presses.subtracting(consumed)
    if !remainder.isEmpty { super.pressesBegan(remainder, with: event) }
  }
}
