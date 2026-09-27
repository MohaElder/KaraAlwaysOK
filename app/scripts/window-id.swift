// Prints the window number of the first normal on-screen window owned by one of the given processes (names or process ids).
import CoreGraphics

let owners = Set(CommandLine.arguments.dropFirst())
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let owned = { (w: [String: Any]) in
    owners.contains(w[kCGWindowOwnerName as String] as? String ?? "") || owners.contains(String(w[kCGWindowOwnerPID as String] as? Int ?? -1))
}
if let w = windows.first(where: { owned($0) && ($0[kCGWindowLayer as String] as? Int) == 0 }) {
    print(w[kCGWindowNumber as String]!)
}
