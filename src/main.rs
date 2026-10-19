fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    #[test]
    fn works() {
        assert_eq!(1 + 1, 2);
    }
}
