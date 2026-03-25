//
//  Item.swift
//  BridgingIO
//
//  Created by 经典 on 2026/3/25.
//

import Foundation
import SwiftData

@Model
final class Item {
    var timestamp: Date
    
    init(timestamp: Date) {
        self.timestamp = timestamp
    }
}
