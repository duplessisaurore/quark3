<h1 align="center">Quark3</h1>

<div align="center">
⚛️🔬⚡🌌🌀
</div>
<div align="center">
  <strong>A textual version of Lepton3</strong>
</div>
<div align="center">
  A textual assembly language that compiles to <code>Lepton3</code> bytecode. 
</div>


## 🌌 Table of Contents
- [<code>✨ What is Quark3?</code>](#what-is-Quark3)
- [<code>🔭 Community</code>](#community)
- [<code>🔬 Quark3 Language</code>](#quark3-language)
- [<code>🌌 Assembly/Disassembly</code>](#quark3-asm-disasm)
- [<code>🔧 Boson3 Preprocessor</code>](#boson3-preprocessor)
- [<code>🧾 License</code>](#license)
- [<code>🎓 Acknowledgments</code>](#acknowledgements)

<a name="what-is-Quark3"></a>
## ✨ What is Quark3?

`Quark3` is an experimental free and open-source textual assembly language that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.  `Fermion3` aims to be an improvement of the prior `Faerlys` and `Quasar2` languages. As it is part of version 3.0, there is a `3` at the end.

<a name="community"></a>
## 🔭 Community

Before contributing or participating in discussions with the community, you should familiarize yourself with our [**Code of Conduct**](./CODE_OF_CONDUCT.md).

* **[Discord](https://discord.gg/wXzj2cqZ3Q):** Fermion3's official discord server.

If there are any other communities that should be added to the list, please make a PR.

If you'd like to help build Quark3, check out the **[Contributor's Guide](./CONTRIBUTING.md)**.

<a name="quark3-language"></a>
## 🔬 Quark3 Language

The `Quark3` assembler takes a textual source code file in and outputs a `Lepton3` bytecode image that can be executed by the `Lepton3` virtual machine. Each `Quark3` file is formed from *directives* and *instructions* which can produce any possible `Lepton3` bytecode image. 

A comment in `Quark3` begins with `//`, any following characters on the same line will be ignored by the parser.

### Directives

Directives in the `Quark3` language begin with a `@` symbol. These allow us to define the entry point of the image, objects, functions and various other components of a `Lepton3` image as will be explained.

---

### @entry \<name>

This directive defines the **entry point** of the image, that is, the function in which execution begins. 

This takes the *name* of the function as specified by a function directive which is automatically mapped to it's index on assembly.

**Example**:

```
@entry main

@fn main 0 0
    push.unit
    return
```

---

### @object \<name> \<fields>

This directive defines a new entry in the **object table** of the image.

- The *name* of the object can be referred to by the `ObjectNew` `Quark3` instruction to construct this specific object.

- *fields* specifies how many fields this object contains.

**Example**:

```
@object Point 2

@fn new_point 2 2

    // load.local arguments to function
    push.uint 0
    load.local
    push.uint 1
    load.local

    // Make Point (two fields) from the locals
    object.new Point

    return 
```

---

### @fn \<name> \<args> \<locals>

This directive defines a new entry in the **function table** of the image.

- The *name* of the function can be referred to by the `Call` and `TailCall` `Quark3` instructions to call this specific function at any point.

- *args* specifies how many arguments this function takes.

- *locals* specifics how large the locals of this function should be, it must exceed *args* as *args* are copied into the first n args locals in `Lepton3`


**Example**:

```
// A function that takes two arguments
@fn my_function 2 2

    // load.local arguments to function into stack
    push.uint 0
    load.local
    push.uint 1
    load.local
    
    return 
```

---

### @loc \<file> \<src_line> \<src_col>

This directive defines a new entry in the **debug info** of the image.

- The *file* of the location defines the source file which is being linked to at this location for debugging purposes

- *src_line* specifies the source line of code in the *file* that these instructions were generated from.

- *src_col* specifies the source line of code in the *file* that these instructions were generated from.

**Example**:

```
@fn new_point 2 2
    push.uint 0
    load.local
    push.uint 1
    load.local

    // These instructions (matches closest loc)
    // came from "my_file.qk3" at line 25 col 0
    @loc "my_file.qk3" 25 0
    
    object.new Point
    return 
```

## Labels

To make writing `Jump`/`JumpIfTrue`/`JumpIfFalse` and `Try` instructions easier, such that the offset from the function instruction base does not need to be manually calculated, `Quark3` provides **labels**.

These are defined by inserting a:

```
<label>:
```

into some function's body, for example:

```
@fn count_down 1 1

    push.uint 0
    load.local

    push.int 0
    int.equal

    jump.if.true done

    push.uint 0
    load.local

    push.int 1
    int.sub

    tail.call count_down

done:

    push.int 0
    return
```

The \<label> can be referred to by offset-based instructions in `Quark3` instead of needing to manually calculate the offset.

## Instructions

Opcodes and instructions in `Quark3` are textual and sometimes inlined versions of those found in `Lepton3`. 

They serve the exact same purpose as the `Lepton3` instructions, but with some sugar attached. 

There are two available forms:

### Stack Operations

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| push.int | pin | PushInt |
| push.uint | pui | PushUInt |
| push.float | pfl | PushFloat |
| push.bool | pbl | PushBool |
| push.unit | pun | PushUnit |
| duplicate | dup | Duplicate |
| pop | pop | Pop |
| swap | swp | Swap |

For the `PushInt`/`PushUInt`/`PushFloat`/`PushBool` instructions, the operand is inlined as follows:

```
// Pushes the constant integer '7' onto the stack
push.int 7

// Pushes the constant boolean true onto the stack
push.bool true

// Pushes the constant float 6.7 onto the stack
push.float 6.7

// Pushes the constant unsigned int '7' onto the stack
push.uint 7
```

For booleans the accepted constnat values are `1`/`true` or `0`/`false`.

### Integer Arithmetic

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| numeric.add | nad | Add |
| numeric.sub | nsb | Sub |
| numeric.mul | nml | Mul |
| int.div | idv | Div |
| int.mod | imd | Mod |
| int.neg | ing | Neg |
| uint.div | udv | UDiv |
| uint.mod | umd | UMod |

### Bitwise Operations

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| numeric.shift.left | nsl | ShiftL |
| int.shift.right | isr | ShiftR |
| uint.shift.right | usr | UShiftR |
| bitwise.and | and | And |
| bitwise.or | orr | Or |
| bitwise.xor | xor | Xor |
| bitwise.not | not | Not |

### Integer Comparison

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| numeric.equal | neq | Equal |
| numeric.not.equal | nnq | NotEqual |
| int.less.than | ilt | LessThan |
| int.less.than.equal | ile | LessThanEq |
| int.greater.than | igt | GreaterThan |
| int.greater.than.equal | ige | GreaterThanEq |
| uint.less.than | ult | ULessThan |
| uint.less.than.equal | ule | ULessThanEq |
| uint.greater.than | ugt | UGreaterThan |
| uint.greater.than.equal | uge | UGreaterThanEq |

### Boolean Operations

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| bool.and | ban | BoolAnd |
| bool.or | bor | BoolOr |
| bool.not | bnt | BoolNot |

### Control Flow

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| jump | jmp | Jump |
| jump.if.true | jit | JumpIfTrue |
| jump.if.false | jif | JumpIfFalse |
| call | cal | Call |
| tail.call | tcl | TailCall |
| return | ret | Return |
| abort | abt | Abort |

For the `Call`/`TailCall` instruction, the function is not referred to by its index into the function table, but by the sugared \<name> defined by the `@fn` directive. This is inlined as follows:


```
@fn my_fn 0 0
    push.unit
    return

// ...
// In some other function we
// can refer to my_fn in call/tail.call using
// its name

    call my_fn
    tail.call my_fn
```

For the `Jump`/`JumpIfTrue`/`JumpIfFalse`/`Try` instructions, the offset is not calculated or even passed inline, instead a defined \<label> which will be jumped to is supplied. This is inlined as follows:


```
@fn my_fn 0 0
    // Jump to the go_return label
    jump go_return

    // This will not be ran!
    abort

go_return:
    push.unit
    return
```

### Locals & Globals

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| load.local | lol | Load |
| store.local | stl | Store |
| load.global | log | LoadGlobal |
| store.global | stg | StoreGlobal |

### Arrays

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| array.new | anw | ArrayNew |
| array.cons | acs | ArrayCons |
| array.head | ahd | ArrayHead |
| array.tail | atl | ArrayTail |
| array.length | aln | ArrayLength |
| array.nth | nth | ArrayNth |
| array.append | aap | ArrayAppend |
| array.set | ast | ArraySet |

### Objects

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| object.new | onw | ObjectNew |
| object.set | ost | ObjectSet |
| object.get | ogt | ObjectGet |
| object.length | oln | ObjectLength |
| object.type.tag | ott | ObjectTypeTag |

For the `ObjectNew` instruction, the object is not referred to by its index, but by the sugared \<name> defined by the `@object` directive. This is inlined with the opcode as follows:

```
// Defines the object type
@object Nothing 0

// Create a new object of the type "Nothing"
object.new Nothing
```

### Tagged Values

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| tag.new | tnw | TagNew |
| tag.equal | teq | TagEq |

### Capabilities

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| call.cap | cap | CallCap |

### Try/Raise

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| try | try | Try |
| end.try | etr | EndTry |
| raise | rse | Raise |

### Floating Point Arithmetic

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| float.add | fad | FAdd |
| float.sub | fsb | FSub |
| float.mul | fml | FMul |
| float.div | fdv | FDiv |
| float.mod | fmd | FMod |
| float.neg | fng | FNeg |

### Floating Point Comparison

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| float.equal | feq | FEqual |
| float.not.equal | fnq | FNotEqual |
| float.less.than | flt | FLessThan |
| float.less.than.equal | fte | FLessThanEq |
| float.greater.than | fgt | FGreaterThan |
| float.greater.than.equal | fge | FGreaterThanEq |
| float.is.nan | fin | FIsNaN |

### Types and conversions

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| int.to.float | itf | IntToFloat |
| float.to.int | fti | FloatToInt |
| uint.to.float | utf | UIntToFloat |
| float.to.uint | ftu | FloatToUInt |
| int.to.uint | itu | IntToUInt |
| uint.to.int | uti | UIntToInt |
| type.of | tof | TypeOf |

### Heap Operations

| Long Form | Short Form | Opcode |
|-----------|------------|---------|
| clone | cln | Clone |

<a name="quark3-asm-disasm"></a>
## 🌌 Assembly/Disassembly

The `quark_std` crate provides a rust-standard supported binary for the `Quark3` assembler.

This assembles the `Quark3` textual source code into a `Lepton3` image. It optionally can also output a **source map** which holds references to all the names used in the source code and where they were used.

The advantages of `Quark3` being so close to the `Lepton3` bytecode, without much sugar on top of just being a literal textual map to the image is that we can easily **disassemble** the image back into `Quark3` source code (excl. whitespace and comments).

This disassembler is provided in binary form by the `quark_std_disasm` crate, similar to `quark_std`. For proper name mapping during disassembly the produce source map can be provided to the disassembler, else the disassembler will choose generic names for all named elements (objects, functions and labels).

<a name="boson3-preprocessor"></a>
## 🔧 Boson3 Preprocessor

Quark3 is very low level and unfriendly. The `Boson3` preprocessor aims to provide a layer above `quark3` which enables for programming in `quark3` at a usable level.

These are the things provided by `boson3` above `quark3`:

### Locals and Global naming

```
// This names the slot 0 global under the alias of <counter>
@global counter 0

// We can then refer to it in a special inline load.global
@fn my_fn 0 0
    load.global counter

// This desugars to
@fn my_fn 0 0
    push.uint 0
    load.global
```

```
// This names the locals as <x, y>
@fn my_fn 2 2 (x, y)
    load.local x
    store.local y
```

### If statements

```
@fn my_fn 2 2 (x, y)
    load.local x

    if
        blah blah blah
    else
        blah blah blah
    end

    blah blah blah
```

This desugars to

```
@fn my_fn 2 2 (x, y)
    load.local x
    
    jump.if.false __if_1_else
    ...true...
    jump __if_1_end

__if_1_else:
        ...false...
__if_1_end:

    blah blah blah
```

The else is optional.

### Loop

```
@fn my_fn 2 2
    loop
        ...body...
    end

    blah blah blah
```

This desugars to 

```
@fn my_fn 2 2
__loop_1_start:
    ...body...
    jump __loop_1_start

__loop_1_end:

    blah blah blah
```

### Object field naming

```
// You can name object fields now!
@object Point 2 (x, y)
```

This can be used like:

```
object.get Point.x
object.set Point.y
```

Which just desugars to:

```
push.uint 0
object.get

push.uint 1
swap
object.set
```

### Named capabilities

Like globals:

```
@capability uart_write 0
@capability gpio_set 1
```

This lets us use:

```
call.cap uart_write
```

Which desugars to

```
push.uint 0
call.cap
```

### Try Catch

Try catch blocks exist too!

```
@fn my_fn 2 2
    try
        ...risky...
    catch
        ...handler...
    end

    blah blah blah
```

desguars to:

```
@fn my_fn 2 2

    try __try_1_handler
    ...risky...
    end.try
    jump __try_1_end

__try_1_handler:
    ...handler...

__try_1_end:
    blah blah blah
```



<a name="license"></a>
## 🧾 License

This repository and all elements of Quark3 are licensed under AGPLv3. See the `LICENSE` file in the repository root.

Quark3 will *always* be free and open-source.

<a name="acknowledgements"></a>
## 🎓 Acknowledgments

- Thanks to ``Lean4``, ``Rust`` & ``Haskell`` for inspiration.
- Thank you for reading this README/Learning about Quark3! 💛
- [No generative AI will ever be used for contributions, see the AI Policy section.](./CONTRIBUTING.md)

<br>

-------------

[**Created by all Contributors to Quark3**](https://github.com/duplessisaurore/Quark3/graphs/contributors?all=1)

Love for everyone 💛 
