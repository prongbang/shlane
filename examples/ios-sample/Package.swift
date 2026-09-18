// swift-tools-version: 5.9
import PackageDescription

// A package rather than an .xcodeproj on purpose: it is a few readable lines
// instead of a generated pbxproj nobody can review, and `xcodebuild test
// -scheme Counter` works against it just the same.
let package = Package(
    name: "Counter",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [
        .library(name: "Counter", targets: ["Counter"])
    ],
    targets: [
        .target(name: "Counter"),
        .testTarget(name: "CounterTests", dependencies: ["Counter"])
    ]
)
