// Prints the window number of the first normal on-screen window owned by one of the given processes.
import CoreGraphics

let owners = Set(CommandLine.arguments.dropFirst())
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
if let w = windows.first(where: { owners.contains($0[kCGWindowOwnerName as String] as? String ?? "") && ($0[kCGWindowLayer as String] as? Int) == 0 }) {
    print(w[kCGWindowNumber as String]!)
}
