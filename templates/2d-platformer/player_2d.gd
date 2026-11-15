class_name Player2DController
extends CharacterBody2D

# 3JSE-inspired 2D Platformer Feel Knobs
@export_range(100.0, 600.0, 10.0) var speed: float = 300.0
@export_range(-800.0, -200.0, 10.0) var jump_velocity: float = -420.0
@export_range(400.0, 2000.0, 20.0) var gravity: float = 980.0
@export_range(0.0, 1.0, 0.05) var coyote_time: float = 0.15

# Internal controller state
var is_grounded: bool = true
var time_since_grounded: float = 0.0

# Gameplay signals
signal jumped()
signal landed()
signal goal_reached()

func _ready() -> void:
    print("Player2DController initialized at position ", position)

func _physics_process(delta: float) -> void:
    # Apply gravity when in air
    if not is_on_floor():
        velocity.y += gravity * delta
        time_since_grounded += delta
        is_grounded = false
    else:
        if not is_grounded:
            is_grounded = true
            emit_signal("landed")
        time_since_grounded = 0.0

func _on_goal_reached(body: Node2D) -> void:
    if body == self:
        emit_signal("goal_reached")
