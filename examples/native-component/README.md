
### `native-component`

Shows how to implement a non-visual element of a `.slint` file in C++ with an experimental `native component`.

`app.slint` declares `PrimeFinder` and instantiates it once per row of a `for` repeater.
`prime_finder.cpp` implements the generated `PrimeFinder` class and defines `PrimeFinder::create()`.
The search runs on the UI thread in small chunks driven by a `slint::Timer`, so no threads are needed.

This example is C++ only.
Build it with the experimental features enabled:

```sh
cmake -S examples/native-component -B build -DSLINT_FEATURE_EXPERIMENTAL=ON
cmake --build build
./build/native_component
```
