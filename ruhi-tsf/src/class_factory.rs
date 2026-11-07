/// RuhI (如喜) Windows COM Class Factory
use std::ffi::c_void;
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;

use crate::service::RuhiTextService;
use crate::logger;

#[implement(IClassFactory)]
pub struct RuhiClassFactory;

impl IClassFactory_Impl for RuhiClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if punkouter.is_some() {
                logger::log("CreateInstance: aggregation not supported");
                return Err(Error::from_hresult(CLASS_E_NOAGGREGATION));
            }
            if ppvobject.is_null() || riid.is_null() {
                logger::log("CreateInstance: invalid null arg");
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            logger::log(&format!("CreateInstance called for IID: {:?}", unsafe { *riid }));

            let service: IUnknown = RuhiTextService::new().into();
            unsafe {
                let res = service.query(riid, ppvobject);
                logger::log(&format!("CreateInstance query returned HRESULT: {:?}", res));
                res.ok()
            }
        })).unwrap_or_else(|_| Err(Error::from_hresult(E_FAIL)))
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}
