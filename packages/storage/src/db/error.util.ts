import { Injectable, Logger } from '@nestjs/common'
import { Prisma } from '@prisma/client'

@Injectable()
export class ErrorUtil {
    private readonly logger = new Logger()

    explain(e: any): string {
        if (e instanceof Prisma.PrismaClientKnownRequestError) {
            this.logger.error(
                `code: ${e.code}, name: ${e.name} ,message: ${
                    e.message
                }, explanation: ${prismaErrorCodeExplain(e.code)})}`
            )
            return prismaErrorCodeExplain(e.code)
        } else {
            this.logger.error(e.message)
            return e.message
        }
    }
}

const prismaErrorCodeExplain = (code: string) => {
    switch (code) {
        case 'P1000':
            return 'Authentication failed against database server'
        case 'P1001':
            return "Can't reach database server"
        case 'P1002':
            return 'The database server was reached but timed out'
        case 'P1003':
            return 'Database does not exist'
        case 'P1008':
            return 'Operations timed out'
        case 'P1009':
            return 'Database already exists on the database server'
        case 'P1010':
            return 'User was denied access on the database'
        case 'P1011':
            return 'Error opening a TLS connection'
        case 'P1012':
            return 'Schema validation error after upgrading Prisma to version 4.0.0'
        case 'P1013':
            return 'The provided database string is invalid'
        case 'P1014':
            return 'The underlying kind for model does not exist'
        case 'P1015':
            return 'Prisma schema is using features that are not supported for the version of the database'
        case 'P1016':
            return 'Raw query had an incorrect number of parameters'
        case 'P1017':
            return 'Server has closed the connection'
        case 'P2000':
            return "The provided value for the column is too long for the column's type"
        case 'P2001':
            return 'The record searched for in the where condition does not exist'
        case 'P2002':
            return 'Unique constraint failed on the constraint'
        case 'P2003':
            return 'Foreign key constraint failed on the field'
        case 'P2004':
            return 'A constraint failed on the database'
        case 'P2005':
            return "The value stored in the database for the field is invalid for the field's type"
        case 'P2006':
            return "The provided value is too large for the column's integer type"
        case 'P2007':
            return 'The provided string value is not a valid enum value'
        case 'P2008':
            return 'The provided value is not a valid json'
        case 'P2009':
            return 'The provided value is not a valid date, time or timestamp'
        case 'P2010':
            return 'The provided field does not exist on the related model'
        case 'P2011':
            return 'The relation represented by the field does not exist'
        case 'P2012':
            return 'The records for the relation field cannot be created or updated because they were not connected'
        case 'P2013':
            return 'The record for the nested relation field cannot be created because it is missing in the nested create data'
        case 'P2014':
            return 'The records for the nested relation field cannot be updated because they are missing in the nested update data'
        case 'P2015':
            return 'The records for the nested relation field cannot be upserted because they are missing in the nested upsert data'
        case 'P2016':
            return 'The required connected records were not provided for the relation field'
        case 'P2017':
            return 'Connected records were not found for the relation field'
        case 'P2018':
            return 'The required connected records were not provided for the scalar field'
        case 'P2019':
            return 'The connected record was not found for the scalar field'
        case 'P2020':
            return 'The provided value for the relation field is not valid'
        case 'P2021':
            return 'The provided value for the scalar field is not valid'
        case 'P2022':
            return 'The record cannot be updated because it would cause a unique constraint violation'
        case 'P2023':
            return 'The record cannot be deleted because it is referenced by other records'
        case 'P2024':
            return 'The record cannot be deleted because it would cause a relation constraint violation'
        case 'P2025':
            return 'The record cannot be updated because it would cause a relation constraint violation'
        case 'P2026':
            return 'The upsert operation failed because it would cause a unique constraint violation'
        case 'P2027':
            return 'The upsert operation failed because it would cause a relation constraint violation'
        case 'P2028':
            return 'Transaction API error'
        case 'P2030':
            return 'Cannot find a fulltext index to use for the search, try adding a @@fulltext([Fields...]) to your schema'
        case 'P2031':
            return 'Prisma needs to perform transactions, which requires your MongoDB server to be run as a replica set. See details: https://pris.ly/d/mongodb-replica-set'
        case 'P2033':
            return "A number used in the query does not fit into a 64 bit signed integer. Consider using BigInt as field type if you're trying to store large integers"
        case 'P2034':
            return 'Transaction failed due to a write conflict or a deadlock. Please retry your transaction'
        // Data Proxy
        case 'P5000':
            return 'This request could not be understood by the server'
        case 'P5001':
            return 'This request must be retried'
        case 'P5002':
            return 'The datasource provided is invalid: Could not parse the URL of the datasource. Datasource URL must use prisma:// protocol when --data-proxy is used. No valid API key found'
        case 'P5003':
            return 'Requested resource does not exist'
        case 'P5004':
            return 'The feature is not yet implemented: beforeExit event is not yet supported'
        case 'P5005':
            return 'Schema needs to be uploaded'
        case 'P5006':
            return 'Unknown server error'
        case 'P5007':
            return 'Unauthorized, check your connection string'
        case 'P5008':
            return 'Usage exceeded, retry again later'
        case 'P5009':
            return 'Request timed out'
        case 'P5010':
            return 'Cannot fetch data from service'
        case 'P5011':
            return 'Request parameters are invalid'
        case 'P5012':
            return 'Engine version is not supported'
        case 'P5013':
            return 'Engine not started: healthcheck timeout'
        case 'P5014':
            return 'Unknown engine startup error (contains message and logs)'
        case 'P5015':
            return 'Interactive transaction error: Could not parse interactive transaction ID. Could not find Query Engine for the specified host and transaction ID. Could not start interactive transaction'
        default:
            return 'Unknown error code'
    }
}
