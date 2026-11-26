# Third-party notices

## OpenMU

Network packet layouts and the Season 6 monster definitions and spawns in `crates/mu-server/datos`
were taken from [OpenMU](https://github.com/MUnique/OpenMU) (`src/Network/Packets/*.xml`,
`src/Persistence/Initialization`, see `tools/gen_spawns.py`). OpenMU is distributed under the MIT
licence:

```
Copyright 2017 MUnique

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

## Reference projects

The client logic was studied from the projects listed in the README ("References and
inspiration"). Their files are not included in this repository.

## Rust crates

Dependencies such as [Bevy](https://github.com/bevyengine/bevy) (MIT/Apache-2.0) are downloaded
by Cargo and keep their own licences.
