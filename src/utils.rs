
pub(crate) fn check_err_lt<T: Ord + Default>(num: T) -> std::io::Result<T> {
    if num < T::default() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(num)
}

pub(crate) fn check_err_ne<T: Ord + Default>(num: T) -> std::io::Result<T> {
    if num != T::default() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(num)
}