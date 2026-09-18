import XCTest
@testable import Counter

final class CounterTests: XCTestCase {
    func testStartsAtZero() {
        XCTAssertEqual(Counter().value, 0)
    }

    func testIncrements() {
        var counter = Counter()
        counter.increment()
        counter.increment(by: 4)
        XCTAssertEqual(counter.value, 5)
    }

    func testDecrementStopsAtZero() {
        var counter = Counter(value: 1)
        counter.decrement()
        counter.decrement()
        XCTAssertEqual(counter.value, 0, "a counter should not go negative")
    }

    func testNegativeStartIsClamped() {
        XCTAssertEqual(Counter(value: -5).value, 0)
    }

    func testReset() {
        var counter = Counter(value: 9)
        counter.reset()
        XCTAssertEqual(counter.value, 0)
    }

    func testDescriptionIsSingularForOne() {
        XCTAssertEqual(Counter(value: 1).description, "1 item")
        XCTAssertEqual(Counter(value: 2).description, "2 items")
    }
}
