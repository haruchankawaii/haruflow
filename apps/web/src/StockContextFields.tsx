import {FormField,FormError} from "./components";
import {useResource} from "./hooks";
import type {InventorySettings,StockContextInput,StockLot,Warehouse} from "./inventory-model";

export function StockContextFields({productId,value,onChange,incoming=false,disabled=false}:{productId:string;value:StockContextInput;onChange:(value:StockContextInput)=>void;incoming?:boolean;disabled?:boolean}){
 const warehouses=useResource<{items:Warehouse[]}>("/inventory/warehouses");
 const lots=useResource<{items:StockLot[]}>(productId?`/inventory/lots?product_id=${productId}`:"/inventory/lots");
 const settings=useResource<InventorySettings>(`/inventory/products/${productId||"00000000-0000-0000-0000-000000000000"}/settings`);
 const options=warehouses.data?.items.filter(w=>w.active)??[];const defaultWarehouse=options.find(w=>w.is_default)?.id;const serial=incoming&&settings.data?.tracking==="serial";
 return <div className="stock-context-fields"><FormField label="Gudang"><select aria-label="Gudang persediaan" value={value.warehouse_id??""} disabled={disabled||warehouses.loading} onChange={e=>onChange({...value,warehouse_id:e.target.value||undefined,allocations:[]})}><option value="">Gudang utama</option>{options.map(w=><option key={w.id} value={w.id}>{w.code} · {w.name}</option>)}</select></FormField>
 {productId&&<><FormField label={serial?"Nomor seri yang diterima":"Lot / nomor seri"} hint={serial?"Pilih satu seri untuk setiap unit; jumlah alokasi harus sama dengan jumlah barang.":incoming?"Wajib diisi untuk produk dengan pelacakan lot/seri.":"Pilihan otomatis menggunakan lot belum kedaluwarsa berdasarkan kedaluwarsa terdekat."}>
 {serial?<select multiple aria-label="Nomor seri diterima" value={(value.allocations??[]).map(a=>a.lot_id??"")} disabled={disabled||lots.loading} onChange={e=>onChange({...value,lot_id:undefined,allocations:[...e.target.selectedOptions].map(o=>({warehouse_id:value.warehouse_id||defaultWarehouse||"",lot_id:o.value,quantity:"1"}))})}>{(lots.data?.items??[]).filter(l=>l.kind==="serial").map(l=><option key={l.id} value={l.id}>{l.code}{l.expires_on?` · ${l.expires_on}`:""}</option>)}</select>:<select aria-label="Lot persediaan" value={value.lot_id??""} disabled={disabled||lots.loading} onChange={e=>onChange({...value,lot_id:e.target.value||undefined,allocations:[]})}><option value="">{incoming?"Tanpa lot / seri":"Pilih otomatis (FEFO)"}</option>{(lots.data?.items??[]).map(l=><option key={l.id} value={l.id}>{l.code}{l.expires_on?` · ${l.expires_on}`:""}</option>)}</select>}
 </FormField><FormError error={warehouses.error||lots.error||settings.error}/></>}
 </div>;
}
